use std::ffi::CString;

use esp_idf_svc::{
    sys::{
        esp, esp_eap_client_set_identity, esp_eap_client_set_password, esp_eap_client_set_username,
        esp_wifi_sta_enterprise_enable,
    },
    wifi::{BlockingWifi, ClientConfiguration, EspWifi},
};

pub fn init_enterprise(ssid: heapless::String<32>, wifi: &mut BlockingWifi<EspWifi<'static>>) {
    wifi.set_configuration(&esp_idf_svc::wifi::Configuration::Client(
        ClientConfiguration {
            auth_method: esp_idf_svc::wifi::AuthMethod::WPA2Enterprise,
            ssid,
            ..Default::default()
        },
    ))
    .unwrap();

    log::info!("Setting WPA2Enterprise params");
    let identity = CString::new(include_str!("../username.txt")).unwrap();
    esp!(unsafe {
        esp_eap_client_set_identity(identity.as_ptr().cast(), identity.as_bytes().len() as i32)
    })
    .unwrap();

    let username = CString::new(include_str!("../username.txt")).unwrap();
    esp!(unsafe {
        esp_eap_client_set_username(username.as_ptr().cast(), username.as_bytes().len() as i32)
    })
    .unwrap();

    let password = CString::new(include_str!("../password.txt")).unwrap();
    esp!(unsafe {
        esp_eap_client_set_password(password.as_ptr().cast(), password.as_bytes().len() as i32)
    })
    .unwrap();

    esp!(unsafe { esp_wifi_sta_enterprise_enable() }).unwrap();

    log::info!("Starting wifi.");
    wifi.start().unwrap();

    log::info!("Connecting wifi.");
    wifi.connect().unwrap();

    log::info!("Waiting for netif.");
    wifi.wait_netif_up().unwrap();
}

pub fn init(
    ssid: heapless::String<32>,
    password: heapless::String<64>,
    wifi: &mut BlockingWifi<EspWifi<'static>>,
) {
    wifi.set_configuration(&esp_idf_svc::wifi::Configuration::Client(
        ClientConfiguration {
            auth_method: esp_idf_svc::wifi::AuthMethod::WPA2Personal,
            ssid,
            password,
            ..Default::default()
        },
    ))
    .unwrap();

    log::info!("Starting wifi.");
    wifi.start().unwrap();

    log::info!("Connecting wifi.");
    wifi.connect().unwrap();

    log::info!("Waiting for netif.");
    wifi.wait_netif_up().unwrap();
}
