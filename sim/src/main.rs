use std::{
    io::{BufReader, Cursor},
    sync::{mpsc, Arc},
    time::{Duration, Instant},
};

use common::{Playing, SimpleArtist, SimpleTrack};
use embedded_graphics_simulator::{
    BinaryColorTheme, OutputSettingsBuilder, SimulatorDisplay, Window,
};

use embedded_graphics::{pixelcolor::Rgb565, prelude::*};
use graphics::IMAGE_WIDTH;
use image::{DynamicImage, GenericImageView};
use ureq::Request;

#[derive(Debug, Clone)]
enum Message {
    /// Sent to update currently playing song, with image if its different, bool indicates if it updated
    UpdateSong(Option<Playing>, Option<Arc<[u8]>>, bool),
    UpdateProgress(u32),
    /// Sent when it is time to scroll text, whatever one is ready
    ScrollText,
}

fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;
    let mut display: SimulatorDisplay<Rgb565> = SimulatorDisplay::new(Size::new(128, 160));

    let output_settings = OutputSettingsBuilder::new()
        .theme(BinaryColorTheme::Default)
        .max_fps(60)
        .build();

    let (sender, receiver) = mpsc::channel::<Message>();

    std::thread::spawn({
        let sender = sender.clone();
        move || loop {
            std::thread::sleep(Duration::from_millis(30));
            sender.send(Message::ScrollText).unwrap();
        }
    });

    std::thread::spawn::<_, color_eyre::Result<()>>(move || {
        let mut last_song = None::<String>;
        let mut image_cache = None::<Arc<[u8]>>;

        loop {
            let playing = ureq::get(
                "https://6q7btxffqgoyulwyg4jktyayzu0kvcyf.lambda-url.us-east-1.on.aws/playing",
            )
            .call()?
            .into_json::<Option<Playing>>()
            .ok()
            .and_then(|p| p);

            if let Some(playing) = playing {
                match &playing.playing.image_url {
                    Some(url)
                        if image_cache.is_none()
                            || !last_song
                                .as_ref()
                                .is_some_and(|last| &playing.playing.name == last) =>
                    {
                        let res = ureq::get(url).call()?;
                        let length = res.header("content-length").unwrap().parse::<usize>()?;

                        let image_type = res
                            .header("content-type")
                            .unwrap()
                            .split_once("/")
                            .unwrap()
                            .1
                            .to_string();

                        let mut buf = Vec::with_capacity(length);
                        res.into_reader().read_to_end(&mut buf)?;

                        let image = match image_type.as_str() {
                            "png" => DynamicImage::from_decoder(
                                image::codecs::png::PngDecoder::new(Cursor::new(&buf)).unwrap(),
                            ),
                            "jpeg" | "jpg" => DynamicImage::from_decoder(
                                image::codecs::jpeg::JpegDecoder::new(Cursor::new(&buf)).unwrap(),
                            ),
                            _ => panic!("Unsupported image type: {image_type}"),
                        }?;

                        let rgb8_image = image
                            .resize(
                                IMAGE_WIDTH,
                                IMAGE_WIDTH,
                                image::imageops::FilterType::Triangle,
                            )
                            .into_rgb8();

                        image_cache = Some(Arc::from(graphics::rgb8_to_rgb565(&rgb8_image)))
                    }
                    _ => {}
                };

                let changed = !last_song
                    .as_ref()
                    .is_some_and(|s| s == &playing.playing.name);
                last_song = Some(playing.playing.name.clone());
                sender
                    .send(Message::UpdateSong(
                        Some(playing),
                        image_cache.clone(),
                        changed,
                    ))
                    .unwrap();

                for i in 1..=5 {
                    std::thread::sleep(Duration::from_secs(1));
                    sender.send(Message::UpdateProgress(i)).unwrap();
                }
            } else {
                sender
                    .send(Message::UpdateSong(None, None, last_song.is_some()))
                    .unwrap();
                last_song = None;
                std::thread::sleep(Duration::from_secs(5));
            }
        }
    });

    let mut window = Window::new("Layout example", &output_settings);
    window.update(&display);

    let mut shifting_title = true;
    let mut title_shift = 0;
    let mut composer_shift = 0;
    let mut curr_playing = None::<Playing>;
    let mut changed_at = Instant::now();

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
                            changed_at = Instant::now();

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
                        curr_playing = Some(playing);
                    } else {
                        curr_playing = None;
                        graphics::draw_no_song(&mut display);
                    }
                }
                Message::UpdateProgress(offset) => {
                    if let Some(playing) = &curr_playing {
                        graphics::draw_current_progress(
                            &mut display,
                            playing.progress_secs + offset,
                            playing.playing.duration,
                        );
                    }
                }
                // Only scroll text after 3 seconds since
                Message::ScrollText if changed_at.elapsed() > Duration::from_secs(3) => {
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
                            }
                        }
                    }
                }
                _ => {}
            },
            Err(_) => {
                for event in window.events() {
                    match event {
                        embedded_graphics_simulator::SimulatorEvent::Quit => std::process::exit(0),
                        _ => {}
                    }
                }
            }
        }

        window.update(&display);
    }
    Ok(())
}
