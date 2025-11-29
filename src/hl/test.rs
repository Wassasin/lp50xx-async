use embedded_hal_mock::eh1::i2c::{Mock, Transaction};

use crate::{
    hl::{self, Address, Config, LP50xx},
    ll,
};

const ADDRESS: u8 = 0b0110_0010;

fn regw(register: u8, values: &[u8]) -> Transaction {
    let mut expected = vec![register];
    expected.extend_from_slice(values);
    Transaction::write(ADDRESS, expected)
}

fn regr(register: u8, values: &[u8]) -> Transaction {
    Transaction::write_read(ADDRESS, vec![register], Vec::from(values))
}

#[async_std::test]
async fn lp5030() {
    let expectations = [
        regw(0x00, &[0x40]),
        regr(0x01, &[0x3C]),
        regw(0x01, &[0x3C & !(1 << 2) | (1 << 1)]),
        regw(0x08, &[0x55; 10]),
        regw(0x11, &[0x54]),
        regw(0x2F, &[0x01, 0x02, 0x03]),
        regw(0x2A, &[0xFF]),
    ];

    let mut i2c = Mock::new(&expectations);

    let hl = hl::LP5030::new(&mut i2c, Address::Address1);
    let mut hl = hl.enable().await.unwrap();

    hl.configure(&Config {
        log_scale: true,
        power_save: true,
        pwm_dithering: false,
        max_current: ll::MaxCurrentOption::Current35MA,
    })
    .await
    .unwrap();

    hl.set_all_brightness(0x55).await.unwrap();
    hl.set_rgb_brightness(9, 0x54).await.unwrap();
    hl.set_rgb(9, (0x01, 0x02, 0x03)).await.unwrap();
    hl.set_channel(22, 0xFF).await.unwrap();

    i2c.done();
}
