# lp50xx_async
Async Rust driver for the LP50xx family of Texas Instruments I2C RGB LED drivers.

## Supported devices
All devices in the LP50xx family are supported:
* [LP5009](https://www.ti.com/product/LP5009)
* [LP5012](https://www.ti.com/product/LP5012)
* [LP5018](https://www.ti.com/product/LP5018)
* [LP5024](https://www.ti.com/product/LP5024)
* [LP5030](https://www.ti.com/product/LP5030)
* [LP5036](https://www.ti.com/product/LP5036)

## How to use
For any I2C peripheral implementing the [I2c embedded-hal-async trait](https://docs.rs/embedded-hal-async/1.0.0/embedded_hal_async/i2c/trait.I2c.html) you can use this driver as follows:
```rust
let hl = LP5030::new(&mut i2c, Address::Address1);
let mut hl = hl.enable().await.unwrap();

hl.configure(&Config {
    log_scale: true,
    power_save: true,
    pwm_dithering: false,
    max_current: ll::MaxCurrentOption::Current35MA,
})
.await
.unwrap();

// Set all LEDs to the same brightness.
hl.set_all_brightness(0x55).await.unwrap();

// Set a specific LED to a specific brightness.
hl.set_rgb_brightness(9, 0x54).await.unwrap();

// Change the color for that LED.
hl.set_rgb(9, (0x01, 0x02, 0x03)).await.unwrap();

// Set the value for a specific channel (when not used with RGB LEDs).
hl.set_channel(22, 0xFF).await.unwrap();

// Put the device in Standby mode.
let hl = hl.disable().await.unwrap();
```