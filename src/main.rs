mod wifi;

use std::{
    io::BufReader,
    sync::Arc,
    time::{Duration, Instant},
};

use common::Playing;
use embedded_graphics::{pixelcolor::Rgb565, prelude::*};
use esp_idf_svc::{
    eventloop::EspSystemEventLoop,
    hal::{
        delay::Delay,
        gpio::PinDriver,
        prelude::*,
        spi::{config::MODE_3, SpiDeviceDriver, SpiDriverConfig},
    },
    http::client::{Configuration, EspHttpConnection},
    nvs::EspDefaultNvsPartition,
    sys::{esp_crt_bundle_attach, esp_get_free_heap_size},
    wifi::{BlockingWifi, EspWifi},
};
use graphics::IMAGE_WIDTH;
use image::{DynamicImage, ImageBuffer};

use crate::wifi::init_enterprise;

const USE_WPA_ENTERPRISE: bool = true;
const WPA_ENTERPRISE_SSID: &'static str = "eduroam";
const WPA_SSID: &'static str = "GraceHouse";
const WPA_PASSWORD: &'static str = include_str!("../wpa-pass.txt");

#[derive(Debug, Clone)]
enum Message {
    /// Sent to update currently playing song, with image if its different, bool indicates if it updated
    UpdateSong(Option<Playing>, Option<Arc<[u8]>>, bool),
    UpdateProgress,
    /// Sent when it is time to scroll text, whatever one is ready
    ScrollText,
}

fn main() {
    // It is necessary to call this function once. Otherwise some patches to the runtime
    // implemented by esp-idf-sys might not link properly. See https://github.com/esp-rs/esp-idf-template/issues/71
    esp_idf_svc::sys::link_patches();

    // Bind the log crate to the ESP Logging facilities
    esp_idf_svc::log::EspLogger::initialize_default();

    log::info!("Available memory: {}", unsafe { esp_get_free_heap_size() });

    let peripherals = Peripherals::take().unwrap();
    let sclk = peripherals.pins.gpio36;
    let sdo = peripherals.pins.gpio37;
    let sdi = peripherals.pins.gpio35;
    let cs = peripherals.pins.gpio16;

    let config = esp_idf_svc::hal::spi::config::Config::new()
        .baudrate(24.MHz().into())
        .data_mode(MODE_3);

    let spi_driver = SpiDeviceDriver::new_single(
        peripherals.spi2,
        sclk,
        sdo,
        Some(sdi),
        Some(cs),
        &SpiDriverConfig::default(),
        &config,
    )
    .unwrap();

    let dc = PinDriver::output(peripherals.pins.gpio17).unwrap();
    let rst = PinDriver::output(peripherals.pins.gpio18).unwrap();

    log::info!("Creating display dirver.");
    let mut display = st7735_lcd::ST7735::new(spi_driver, dc, rst, true, false, 128, 160);
    display.init(&mut Delay::new_default()).unwrap();

    log::info!("Clearing Display...");
    display.clear(Rgb565::new(0, 0, 0)).unwrap();

    let sysloop = EspSystemEventLoop::take().unwrap();
    let nvs = EspDefaultNvsPartition::take().unwrap();
    let mut wifi = BlockingWifi::wrap(
        EspWifi::new(peripherals.modem, sysloop.clone(), Some(nvs)).unwrap(),
        sysloop,
    )
    .unwrap();

    if USE_WPA_ENTERPRISE {
        init_enterprise(WPA_ENTERPRISE_SSID.try_into().unwrap(), &mut wifi);
    } else {
        wifi::init(
            WPA_SSID.try_into().unwrap(),
            WPA_PASSWORD.try_into().unwrap(),
            &mut wifi,
        );
    }

    const ALBUM_LENGTH: usize = 300;
    let (sender, receiver) = crossbeam_channel::bounded::<Message>(16);

    std::thread::Builder::new()
        .stack_size(8 * 1024)
        .spawn({
            let sender = sender.clone();
            move || loop {
                Delay::new_default().delay_ms(13);
                sender.send(Message::ScrollText).unwrap();
            }
        })
        .unwrap();

    std::thread::Builder::new()
        .stack_size(8 * 1024)
        .spawn({
            let sender = sender.clone();
            move || loop {
                Delay::new_default().delay_ms(1000);
                sender.send(Message::UpdateProgress).unwrap();
            }
        })
        .unwrap();

    std::thread::Builder::new()
        .stack_size(64 * 1024)
        .spawn(move || {
            let mut client = EspHttpConnection::new(&Configuration {
                buffer_size: Some(ALBUM_LENGTH * ALBUM_LENGTH),
                crt_bundle_attach: Some(esp_crt_bundle_attach),
                ..Default::default()
            })
            .unwrap();

            let mut res_buf = vec![0u8; 4 * 1024];
            let mut image_buf = vec![0u8; ALBUM_LENGTH * ALBUM_LENGTH];

            let mut last_song = None::<String>;
            let mut image_cache = None::<Arc<[u8]>>;

            loop {
                let playing = get_playing(&mut client, &mut res_buf);

                if let Err(_) = playing {
                    Delay::new_default().delay_ms(5 * 1000);
                    continue;
                }

                if let Some(playing) = playing.unwrap() {
                    match &playing.playing.image_url {
                        Some(url) => {
                            if image_cache.is_none()
                                || !last_song
                                    .as_ref()
                                    .is_some_and(|last| &playing.playing.name == last)
                            {
                                image_cache = Some(get_image(url, &mut client, &mut image_buf))
                            }
                        }
                        _ => {
                            // If no image_url, don't show image
                            image_cache = None;
                        }
                    };

                    let changed = !last_song
                        .as_ref()
                        .is_some_and(|s| s == &playing.playing.name);
                    last_song = Some(playing.playing.name.clone());
                    let progress = playing.progress_secs;
                    let duration = playing.playing.duration;

                    sender
                        .send(Message::UpdateSong(
                            Some(playing),
                            image_cache.clone(),
                            changed,
                        ))
                        .unwrap();

                    // Simulate progress between requesting next update
                    for i in 1..=5 {
                        Delay::new_default().delay_ms(1000);

                        if progress + i > duration {
                            // Leave early if song over
                            break;
                        }
                    }
                } else {
                    sender
                        .send(Message::UpdateSong(None, None, last_song.is_some()))
                        .unwrap();
                    last_song = None;
                    Delay::new_default().delay_ms(5 * 1000);
                }
            }
        })
        .unwrap();

    let mut shifting_title = true;
    let mut title_shift = 0;
    let mut composer_shift = 0;
    let mut curr_playing = None::<Playing>;
    let mut progress_offset = 0;
    let mut scroll_ended_at = Instant::now();

    loop {
        match receiver.try_recv() {
            Ok(message) => match message {
                Message::UpdateSong(playing, image, changed) => {
                    if let Some(playing) = playing {
                        if changed {
                            // Only redraw image and name on new song
                            shifting_title = true;
                            title_shift = 0;
                            composer_shift = 0;
                            scroll_ended_at = Instant::now();

                            graphics::draw_album_cover(&mut display, image.as_deref());
                            graphics::draw_current_name_and_artist(
                                &mut display,
                                &playing,
                                &mut title_shift,
                                &mut composer_shift,
                            );
                        }

                        graphics::draw_current_progress(
                            &mut display,
                            playing.progress_secs,
                            playing.playing.duration,
                        );
                        progress_offset = 0;
                        curr_playing = Some(playing);
                    } else {
                        curr_playing = None;
                        graphics::draw_no_song(&mut display);
                    }
                }
                Message::UpdateProgress => {
                    if let Some(playing) = &curr_playing {
                        progress_offset += 1;
                        graphics::draw_current_progress(
                            &mut display,
                            playing.progress_secs + progress_offset,
                            playing.playing.duration,
                        );
                    }
                }
                // Only scroll text after 3 seconds since
                Message::ScrollText if scroll_ended_at.elapsed() > Duration::from_secs(3) => {
                    if let Some(playing) = &curr_playing {
                        if shifting_title {
                            title_shift += 1;
                            graphics::draw_current_name_and_artist(
                                &mut display,
                                playing,
                                &mut title_shift,
                                &mut composer_shift,
                            );

                            if title_shift == 0 {
                                shifting_title = false;
                                scroll_ended_at = Instant::now();
                            }
                        } else {
                            composer_shift += 1;
                            graphics::draw_current_name_and_artist(
                                &mut display,
                                playing,
                                &mut title_shift,
                                &mut composer_shift,
                            );

                            if composer_shift == 0 {
                                shifting_title = true;
                                scroll_ended_at = Instant::now();
                            }
                        }
                    }
                }
                _ => {}
            },
            // No events to process
            Err(_) => {}
        }

        // 1ms delay every iteration to make sure good ol watchdog gets fed
        Delay::new_default().delay_ms(1);
    }
}

fn get_playing(client: &mut EspHttpConnection, res_buf: &mut [u8]) -> Result<Option<Playing>, ()> {
    log::info!("Getting currently playing...");
    client
        .initiate_request(
            esp_idf_svc::http::Method::Get,
            "https://6q7btxffqgoyulwyg4jktyayzu0kvcyf.lambda-url.us-east-1.on.aws/playing",
            &[],
        )
        .unwrap();

    client.initiate_response().unwrap();

    let length: usize = client
        .header("content-length")
        .and_then(|s| s.parse().ok())
        .unwrap();

    let mut read = 0;
    while read < length {
        read += client.read(&mut res_buf[read..]).unwrap();
    }

    if !(200..300).contains(&client.status()) {
        log::error!("Bad status requesting current playing: {}", client.status());
        log::error!(
            "Response: {}",
            std::str::from_utf8(&res_buf[..read]).unwrap()
        );

        return Err(());
    }

    log::info!("Deserializing res...");
    Ok(serde_json::from_slice(&res_buf[..read]).unwrap())
}

fn get_image(url: &str, client: &mut EspHttpConnection, image_buf: &mut [u8]) -> Arc<[u8]> {
    client
        .initiate_request(esp_idf_svc::http::Method::Get, url, &[])
        .unwrap();

    client.initiate_response().unwrap();

    let length = client
        .header("content-length")
        .unwrap()
        .parse::<usize>()
        .unwrap();

    let image_type = client
        .header("content-type")
        .unwrap()
        .split_once("/")
        .unwrap()
        .1
        .to_string();

    println!("img type: {image_type}; length: {length}");
    let mut read = 0;
    while read < length {
        read += client.read(&mut image_buf[read..]).unwrap();
    }

    log::info!("Decoding image.");
    let image = match image_type.as_str() {
        "jpeg" | "jpg" => {
            let mut decoder = jpeg_decoder::Decoder::new(BufReader::new(&image_buf[..read]));
            decoder.read_info().unwrap();
            let info = decoder.info().unwrap();

            DynamicImage::ImageRgb8(
                ImageBuffer::from_vec(
                    info.width as u32,
                    info.height as u32,
                    decoder.decode().unwrap(),
                )
                .unwrap(),
            )
        }
        _ => panic!("Unsupported image type: {image_type}"),
    };

    log::info!("Decoded image, resizing.");
    let rgb8_image = image
        .resize(
            IMAGE_WIDTH,
            IMAGE_WIDTH,
            image::imageops::FilterType::Triangle,
        )
        .into_rgb8();

    Arc::from(graphics::rgb8_to_rgb565(&rgb8_image))
}
