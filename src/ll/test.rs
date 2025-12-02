use embedded_hal_mock::eh1::i2c::{Mock, Transaction};

use crate::ll;

const ADDRESS: u8 = 0b0110_000;

fn regw(register: u8, values: &[u8]) -> Transaction {
    let mut expected = vec![register];
    expected.extend_from_slice(values);
    Transaction::write(ADDRESS, expected)
}

fn regr(register: u8, values: &[u8]) -> Transaction {
    Transaction::write_read(ADDRESS, vec![register], Vec::from(values))
}

#[async_std::test]
async fn base_config() {
    let expectations = [
        regw(0x00, &[0x40]),
        regr(0x01, &[0x3C]),
        regw(0x01, &[0x3C & !(1 << 2)]),
    ];

    let mut i2c = Mock::new(&expectations);

    let mut ll = ll::Device::new(ll::i2c::DeviceInterface::new(&mut i2c, ADDRESS));
    ll.device_config_0()
        .write_async(|w| w.set_chip_en(true))
        .await
        .unwrap();
    ll.device_config_1()
        .modify_async(|w| w.set_pwm_dithering_en(false))
        .await
        .unwrap();

    i2c.done();
}
