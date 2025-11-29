#[cfg(test)]
mod test;

use core::{marker::PhantomData, ops::Deref};
use device_driver::AsyncBufferInterface;
use embedded_hal_async::i2c::I2c;

use crate::ll::{self, DeviceError};

/// I2C address used to address the device.
#[derive(Debug, Default, Clone, Copy)]
pub enum Address {
    /// Devices configured with `GND, GND` on addr0 and addr1.
    #[default]
    Address0,
    /// Devices configured with `GND, VCC` on addr0 and addr1.
    Address1,
    /// Devices configured with `VCC, GND` on addr0 and addr1.
    Address2,
    /// Devices configured with `VCC, VCC` on addr0 and addr1.
    Address3,
    /// Broadcast address to address all similar devices on the I2C bus.
    ///
    /// Is only valid if all devices would respond to the same commands in a similar way.
    /// In other words: if all devices are configured identically.
    Broadcast,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "defmt-03", derive(defmt::Format))]
pub enum Error<T> {
    /// The underlying I2C interface returned an error.
    Interface(T),
    /// A LED or RGB LED was indexed incorrectly.
    ///
    /// For example: when you index RGB LED #11 for the LP5030,
    /// which only has up to RGB LED #9.
    Index,
}

impl<T> From<DeviceError<T>> for Error<T> {
    fn from(value: DeviceError<T>) -> Self {
        match value {
            DeviceError::Interface(e) => Error::Interface(e),
            DeviceError::BufferTooSmall => unreachable!(), // Should never happen.
        }
    }
}

/// Color value for an RGB LED, with each `u8` representing the 8-bit value for
/// the Red, Green and Blue channels.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "defmt-03", derive(defmt::Format))]
pub struct Rgb(pub [u8; 3]);

impl core::ops::Deref for Rgb {
    type Target = [u8; 3];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<[u8; 3]> for Rgb {
    fn from(value: [u8; 3]) -> Self {
        Rgb(value)
    }
}

impl From<(u8, u8, u8)> for Rgb {
    fn from(value: (u8, u8, u8)) -> Self {
        Rgb(value.into())
    }
}

/// Markers used to indicated the typestate of the device.
pub mod marker {
    pub struct Standby;
    pub struct Normal;

    trait Sealed {}

    #[allow(private_bounds)]
    pub trait Marker: Sealed {}

    macro_rules! impl_marker {
        ($struct:ty) => {
            impl Sealed for $struct {}
            impl Marker for $struct {}
        };
    }

    impl_marker!(Standby);
    impl_marker!(Normal);
}

/// High level generic driver for the LP50xx family of devices.
///
/// See [LP50xx] on how to instantiate the device.
///
/// The channels can be configured per OUT and per RGB LED.
/// Bank-mode is not (yet) supported.
pub struct Driver<VARIANT: LP50xx, T: I2c, STATE: marker::Marker> {
    device: ll::Device<ll::i2c::DeviceInterface<T>>,
    marker: PhantomData<VARIANT>,
    state: PhantomData<STATE>,
}

/// Generic configuration for an LP50xx device.
pub struct Config {
    /// Use logarithmic scaling.
    pub log_scale: bool,
    /// Turn IC automatically into power save mode when all LEDs are effectively off (after 30ms).
    pub power_save: bool,
    /// Enable dithering mode, stretching the resolution from 9 bits to 12 bits.
    pub pwm_dithering: bool,
    /// The maximum amount of current for a single LED channel.
    ///
    /// 35mA only valid when `Vcc >= 3.3V`.
    pub max_current: ll::MaxCurrentOption,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            log_scale: true,
            power_save: true,
            pwm_dithering: true,
            max_current: ll::MaxCurrentOption::Current25MA5,
        }
    }
}

impl<VARIANT: LP50xx, T: I2c> Driver<VARIANT, T, marker::Standby> {
    fn new(interface: T, address: Address) -> Self {
        let address = match address {
            Address::Address0 => VARIANT::I2C_ADDRESS_BASE,
            Address::Address1 => VARIANT::I2C_ADDRESS_BASE | 0b010,
            Address::Address2 => VARIANT::I2C_ADDRESS_BASE | 0b100,
            Address::Address3 => VARIANT::I2C_ADDRESS_BASE | 0b110,
            Address::Broadcast => VARIANT::I2C_ADDRESS_BROADCAST,
        };

        Self {
            device: ll::Device::new(ll::i2c::DeviceInterface::new(interface, address)),
            marker: PhantomData,
            state: PhantomData,
        }
    }

    /// Enable the device, turning on the constant current sinks
    /// (if any are configured to have a non-zero duty cycle).
    ///
    /// This will consume up to 10mA of current, unless power saving is enabled.
    pub async fn enable(
        mut self,
    ) -> Result<Driver<VARIANT, T, marker::Normal>, DeviceError<T::Error>> {
        self.device
            .device_config_0()
            .write_async(|w| w.set_chip_en(true))
            .await?;

        Ok(Driver {
            device: self.device,
            marker: PhantomData,
            state: PhantomData,
        })
    }
}

impl<VARIANT: LP50xx, T: I2c> Driver<VARIANT, T, marker::Normal> {
    /// Disable the device, putting it into Standby mode.
    ///
    /// All register values will be retained, but the constant current sinks will no longer
    /// be functional, turning off the LEDs.
    ///
    /// Consumes up to 12uA of current, depending on the device type.
    pub async fn disable(
        mut self,
    ) -> Result<Driver<VARIANT, T, marker::Standby>, DeviceError<T::Error>> {
        self.device
            .device_config_0()
            .write_async(|w| w.set_chip_en(false))
            .await?;

        Ok(Driver {
            device: self.device,
            marker: PhantomData,
            state: PhantomData,
        })
    }
}

impl<VARIANT: LP50xx, T: I2c, MARKER: marker::Marker> Driver<VARIANT, T, MARKER> {
    /// Set the general configuration parameters of the device.
    pub async fn configure(&mut self, config: &Config) -> Result<(), DeviceError<T::Error>> {
        self.device
            .device_config_1()
            .modify_async(|w| {
                w.set_log_scale_en(config.log_scale);
                w.set_max_current_option(config.max_current);
                w.set_power_save_en(config.power_save);
                w.set_pwm_dithering_en(config.pwm_dithering);
            })
            .await?;
        Ok(())
    }

    /// Set the specific OUT channel to a specific color value.
    ///
    /// Will return the [Error::Index] if the device does not have the indexed channel.
    pub async fn set_channel(&mut self, channel_i: u8, value: u8) -> Result<(), Error<T::Error>> {
        if channel_i > VARIANT::LED_COUNT {
            return Err(Error::Index);
        }

        self.device
            .interface()
            .write(VARIANT::OUT_START_ADDRESS + channel_i, &[value])
            .await?;
        Ok(())
    }

    /// Set the RGB LED color values.
    ///
    /// Will return the [Error::Index] if the device does not have the indexed RGB LED.
    pub async fn set_rgb(
        &mut self,
        rgb_i: u8,
        value: impl Into<Rgb>,
    ) -> Result<(), Error<T::Error>> {
        if rgb_i > VARIANT::RGB_COUNT {
            return Err(Error::Index);
        }

        // Note: auto incrementing is enabled.
        self.device
            .interface()
            .write(VARIANT::OUT_START_ADDRESS + rgb_i * 3, value.into().deref())
            .await?;
        Ok(())
    }

    /// Set the brightness of a RGB LED (not the color).
    ///
    /// Will return the [Error::Index] if the device does not have the indexed RGB LED
    pub async fn set_rgb_brightness(
        &mut self,
        rgb_i: u8,
        value: u8,
    ) -> Result<(), Error<T::Error>> {
        if rgb_i > VARIANT::RGB_COUNT {
            return Err(Error::Index);
        }

        self.device
            .interface()
            .write(VARIANT::LED_START_ADDRESS + rgb_i, &[value])
            .await?;
        Ok(())
    }

    /// Set the brightness of all RGB LEDs (not the color) in one call.
    pub async fn set_all_brightness(&mut self, value: u8) -> Result<(), Error<T::Error>> {
        let mut buf: heapless::Vec<u8, 36> = heapless::Vec::new();
        buf.extend(core::iter::repeat_n(value, VARIANT::RGB_COUNT as usize));

        self.device
            .interface()
            .write(VARIANT::LED_START_ADDRESS, &buf)
            .await?;
        Ok(())
    }
}

/// Trait for all variants of the LP50xx family of IC's.
pub trait LP50xx: Sized {
    const LED_COUNT: u8;
    const RGB_COUNT: u8 = Self::LED_COUNT / 3;

    const I2C_ADDRESS_BASE: u8;
    const I2C_ADDRESS_BROADCAST: u8;

    /// Register address of `LED0_BRIGHTNESS`.
    const LED_START_ADDRESS: u8;
    /// Register address of `OUT0_COLOR`.
    const OUT_START_ADDRESS: u8 = Self::LED_START_ADDRESS + Self::RGB_COUNT;

    /// Construct the high level driver for a specific IC variant.
    fn new<T: I2c>(interface: T, address: Address) -> Driver<Self, T, marker::Standby> {
        Driver::new(interface, address)
    }
}

pub struct LP5009;
pub struct LP5012;
pub struct LP5018;
pub struct LP5024;
pub struct LP5030;
pub struct LP5036;

impl LP50xx for LP5009 {
    const LED_COUNT: u8 = 9;
    const I2C_ADDRESS_BASE: u8 = 0b0110_0000;
    const I2C_ADDRESS_BROADCAST: u8 = 0b0011_1000;
    const LED_START_ADDRESS: u8 = 0x07;
    const OUT_START_ADDRESS: u8 = 0x0b;
}

impl LP50xx for LP5012 {
    const LED_COUNT: u8 = 12;
    const I2C_ADDRESS_BASE: u8 = 0b0110_0000;
    const I2C_ADDRESS_BROADCAST: u8 = 0b0011_1000;
    const LED_START_ADDRESS: u8 = 0x07;
}

impl LP50xx for LP5018 {
    const LED_COUNT: u8 = 18;
    const I2C_ADDRESS_BASE: u8 = 0b0101_0000;
    const I2C_ADDRESS_BROADCAST: u8 = 0b0111_1000;
    const LED_START_ADDRESS: u8 = 0x07;
    const OUT_START_ADDRESS: u8 = 0x0f;
}

impl LP50xx for LP5024 {
    const LED_COUNT: u8 = 24;
    const I2C_ADDRESS_BASE: u8 = 0b0101_0000;
    const I2C_ADDRESS_BROADCAST: u8 = 0b0111_1000;
    const LED_START_ADDRESS: u8 = 0x07;
}

impl LP50xx for LP5030 {
    const LED_COUNT: u8 = 30;
    const I2C_ADDRESS_BASE: u8 = 0b0110_0000;
    const I2C_ADDRESS_BROADCAST: u8 = 0b0011_1000;
    const LED_START_ADDRESS: u8 = 0x08;
    const OUT_START_ADDRESS: u8 = 0x14;
}

impl LP50xx for LP5036 {
    const LED_COUNT: u8 = 36;
    const I2C_ADDRESS_BASE: u8 = 0b0110_0000;
    const I2C_ADDRESS_BROADCAST: u8 = 0b0011_1000;
    const LED_START_ADDRESS: u8 = 0x08;
}
