use std::{fmt::Debug, sync::OnceLock};

use common::Playing;
use embedded_canvas::{Canvas, CanvasAt};
use embedded_graphics::{
    geometry::{Point, Size},
    image::{Image, ImageRawBE},
    mono_font::{
        jis_x0201::{FONT_6X13, FONT_7X14},
        MonoTextStyleBuilder,
    },
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{Circle, Primitive, PrimitiveStyle, Rectangle},
    text::Text,
    Drawable,
};
use embedded_layout::{
    align::{horizontal, vertical, Align},
    layout::linear::LinearLayout,
    object_chain::Chain,
    View,
};
use embedded_text::{style::TextBoxStyleBuilder, TextBox};
use image::{ImageBuffer, Rgb};
use unicode_segmentation::UnicodeSegmentation;

static KATAKANA_HALF: OnceLock<Vec<Box<str>>> = OnceLock::new();
static KATAKANA_FULL: OnceLock<Vec<Box<str>>> = OnceLock::new();
pub const SCREEN_WIDTH: u32 = 128;
pub const SCREEN_HEIGHT: u32 = 160;
pub const IMAGE_WIDTH: u32 = 114;

pub fn rgb8_to_rgb565(image: &ImageBuffer<Rgb<u8>, Vec<u8>>) -> Vec<u8> {
    // Result is 2/3 of the size of the original
    let mut result = vec![0u8; (image.as_raw().len() as f32 * (2. / 3.)).round() as usize];

    for (i, Rgb([r, g, b])) in image.pixels().copied().enumerate() {
        let rgb565 = rgb565::Rgb565::from_rgb888_components(b, g, r);
        let [first, second] = rgb565.to_bgr565_be();

        result[i * 2] = first;
        result[(i * 2) + 1] = second;
    }

    result
}

const CURRENT_TRACK_HEIGHT: u32 = IMAGE_WIDTH + 14 + 13;

pub fn draw_album_cover<D: DrawTargetExt<Color = Rgb565>>(display: &mut D, image: Option<&[u8]>)
where
    D::Error: Debug,
{
    let display_area = display.bounding_box();

    let image = if let Some(image) = image {
        ImageRawBE::<Rgb565>::new(image, IMAGE_WIDTH)
    } else {
        ImageRawBE::new(&[], IMAGE_WIDTH)
    };

    let chain = Chain::new(Image::new(&image, Point::zero()));

    let mut canvas = Canvas::<Rgb565>::new(Size::new(display_area.size.width, IMAGE_WIDTH));
    canvas.clear(Rgb565::new(0, 0, 0)).unwrap();

    LinearLayout::vertical(chain)
        .with_alignment(horizontal::Center)
        .arrange()
        .align_to(
            &Rectangle::new(Point::zero(), canvas.size()),
            horizontal::Center,
            vertical::Top,
        )
        .draw(&mut canvas)
        .unwrap();

    // Draw all changes at once
    let canvas = canvas.place_at(Point::zero());
    draw_canvas_with_background(canvas, Rgb565::new(0, 0, 0), display);
}

const TEXT_HEIGHT: u32 = 14 + 13;

/// Sets the shift to 0 when it has no effect (it has made a full wrap)
pub fn draw_current_name_and_artist<D: DrawTargetExt<Color = Rgb565>>(
    display: &mut D,
    playing: &Playing,
    title_shift: &mut u32,
    composer_shift: &mut u32,
) where
    D::Error: Debug,
{
    let Playing { playing, .. } = playing;

    let area = Rectangle::new(
        Point::new(0, IMAGE_WIDTH as i32),
        Size::new(display.bounding_box().size.width - 4, TEXT_HEIGHT),
    );

    let name_text_style = MonoTextStyleBuilder::new()
        .font(&FONT_7X14)
        .text_color(Rgb565::new(255, 255, 255))
        .build();
    let composer_text_style = MonoTextStyleBuilder::new()
        .font(&FONT_6X13)
        .text_color(Rgb565::new(255, 255, 255))
        .build();

    let name_bounds = Rectangle::new(Point::zero(), Size::new(area.size.width, 14));
    let composer_bounds = Rectangle::new(Point::zero(), Size::new(area.size.width, 13));

    let mut playing_half = full_katakana_to_half_katakana(&playing.name);
    let mut composer_half = playing
        .artists
        .iter()
        .map(|artist| full_katakana_to_half_katakana(artist.name.as_str()))
        .collect::<Vec<_>>()
        .join(", ");

    let mut name_text = Text::with_baseline(
        &playing_half,
        name_bounds.top_left,
        name_text_style,
        embedded_graphics::text::Baseline::Top,
    );
    let mut composer_text = Text::with_baseline(
        &composer_half,
        composer_bounds.top_left,
        composer_text_style,
        embedded_graphics::text::Baseline::Top,
    );

    let mut canvas = Canvas::<Rgb565>::new(area.size);
    let mut name_canvas = Canvas::<Rgb565>::new(name_text.bounding_box().size());
    name_text.draw(&mut name_canvas).unwrap();

    if name_text.bounding_box().size.width > SCREEN_WIDTH - 4 {
        playing_half.push_str("  ");
        name_text = Text::with_baseline(
            &playing_half,
            name_bounds.top_left,
            name_text_style,
            embedded_graphics::text::Baseline::Top,
        );
        name_canvas = Canvas::<Rgb565>::new(name_text.bounding_box().size());
        name_text.draw(&mut name_canvas).unwrap();

        if *title_shift != 0 {
            // Name text is overflowing, start wrapping pixels it by shift
            let mut new_pixels = vec![None::<Rgb565>; name_canvas.pixels.len()];
            for (og_i, Point { x, y }) in name_canvas.bounding_box().points().enumerate() {
                let new_x = (x - *title_shift as i32)
                    .rem_euclid(name_canvas.bounding_box().size.width as i32);
                let index = point_to_index(
                    name_text.bounding_box().size,
                    Point::zero(),
                    Point::new(new_x, y),
                )
                .unwrap();

                new_pixels[index] = name_canvas.pixels[og_i];
            }
            name_canvas.pixels = new_pixels.into_boxed_slice();

            if *title_shift == name_text.bounding_box().size.width {
                *title_shift = 0;
            }
        }
    } else {
        *title_shift = 0;
    }

    name_canvas
        .crop(&Rectangle::new(
            Point::zero(),
            Size::new(SCREEN_WIDTH - 2, 14),
        ))
        .unwrap()
        .place_at(Point::new(2, 0))
        .draw(&mut canvas)
        .unwrap();

    let mut composer_canvas = Canvas::<Rgb565>::new(composer_text.bounding_box().size());
    composer_text.draw(&mut composer_canvas).unwrap();

    if composer_text.bounding_box().size.width > SCREEN_WIDTH - 4 {
        composer_half.push_str("  ");
        composer_text = Text::with_baseline(
            &composer_half,
            composer_bounds.top_left,
            composer_text_style,
            embedded_graphics::text::Baseline::Top,
        );
        composer_canvas = Canvas::<Rgb565>::new(composer_text.bounding_box().size());
        composer_text.draw(&mut composer_canvas).unwrap();

        if *composer_shift != 0 {
            // Name text is overflowing, start wrapping pixels it by shift
            let mut new_pixels = vec![None::<Rgb565>; composer_canvas.pixels.len()];
            for (og_i, Point { x, y }) in composer_canvas.bounding_box().points().enumerate() {
                let new_x = (x - *composer_shift as i32)
                    .rem_euclid(composer_canvas.bounding_box().size.width as i32);
                let index = point_to_index(
                    composer_text.bounding_box().size,
                    Point::zero(),
                    Point::new(new_x, y),
                )
                .unwrap();

                new_pixels[index] = composer_canvas.pixels[og_i];
            }
            composer_canvas.pixels = new_pixels.into_boxed_slice();

            if *composer_shift == composer_text.bounding_box().size.width {
                *composer_shift = 0;
            }
        }
    } else {
        *composer_shift = 0;
    }

    composer_canvas
        .crop(&Rectangle::new(
            Point::zero(),
            Size::new(SCREEN_WIDTH - 2, 13),
        ))
        .unwrap()
        .place_at(Point::new(2, 14))
        .draw(&mut canvas)
        .unwrap();

    // Draw all changes at once
    let canvas = canvas.place_at(area.top_left);
    draw_canvas_with_background(canvas, Rgb565::new(0, 0, 0), display);
}

const PADDING: u32 = 10;
const CIRCLE_RADIUS: u32 = 4;
const THICKNESS: u32 = 3;

pub fn draw_current_progress<D: DrawTargetExt<Color = Rgb565>>(
    display: &mut D,
    progress_secs: u32,
    duration: u32,
) where
    D::Error: Debug,
{
    let display_area = display.bounding_box();
    let area = Rectangle::new(
        Point::new(0, CURRENT_TRACK_HEIGHT as i32),
        Size::new(
            display_area.size.width,
            display_area.size.height - CURRENT_TRACK_HEIGHT,
        ),
    );
    let bar_width = area.size.width - PADDING * 2;
    // Find percentage of bar and add 2 for padding
    let progress_width = ((bar_width as f32 * ((progress_secs + 1) as f32 / duration as f32))
        .round() as u32)
        .clamp(0, bar_width);

    let progress_width = progress_width.saturating_sub(CIRCLE_RADIUS);
    let remaining_width = bar_width
        .saturating_sub(progress_width)
        .saturating_sub(CIRCLE_RADIUS);

    let mut canvas = Canvas::<Rgb565>::new(area.size);

    canvas.clear(Rgb565::new(0, 0, 0)).unwrap();
    LinearLayout::horizontal(
        Chain::new(
            Rectangle::new(
                Point::zero(),
                Size::new(progress_width.clamp(1, u32::MAX), THICKNESS),
            )
            .into_styled(PrimitiveStyle::with_fill(if progress_width != 0 {
                rgb888_to_rgb565(0x1d, 0xb9, 0x54)
            } else {
                Rgb565::new(0, 0, 0)
            })),
        )
        .append(
            Circle::new(Point::zero(), CIRCLE_RADIUS * 2 + 1).into_styled(
                PrimitiveStyle::with_fill(rgb888_to_rgb565(0x1d, 0xb9, 0x54)),
            ),
        )
        .append(
            Rectangle::new(
                Point::zero(),
                Size::new(remaining_width.clamp(1, u32::MAX), THICKNESS),
            )
            .into_styled(PrimitiveStyle::with_fill(if remaining_width != 0 {
                rgb888_to_rgb565(160, 160, 160)
            } else {
                Rgb565::new(0, 0, 0)
            })),
        ),
    )
    .with_alignment(vertical::Center)
    .arrange()
    .align_to(&canvas.bounding_box(), horizontal::Center, vertical::Center)
    .draw(&mut canvas)
    .unwrap();

    // Draw canvas to display replacing emply pixels with the background color, need this cause canvas slow otherwise
    let canvas = canvas.place_at(area.top_left);
    draw_canvas_with_background(canvas, Rgb565::new(0, 0, 0), display);
}

pub fn draw_no_song<D: DrawTargetExt<Color = Rgb565>>(display: &mut D)
where
    D::Error: Debug,
{
    let display_area = display.bounding_box();

    let text_style = MonoTextStyleBuilder::new()
        .font(&FONT_7X14)
        .text_color(Rgb565::new(255, 255, 255))
        .background_color(Rgb565::new(0, 0, 0))
        .build();

    let text_box_style = TextBoxStyleBuilder::new()
        .height_mode(embedded_text::style::HeightMode::ShrinkToText(
            embedded_text::style::VerticalOverdraw::Hidden,
        ))
        .alignment(embedded_text::alignment::HorizontalAlignment::Center)
        .build();

    let text = TextBox::with_textbox_style("Not Playing", display_area, text_style, text_box_style);

    let mut canvas = Canvas::<Rgb565>::new(display_area.size);

    LinearLayout::vertical(Chain::new(text))
        .with_alignment(horizontal::Center)
        .arrange()
        .align_to(&display_area, horizontal::Center, vertical::Top)
        .draw(&mut canvas)
        .unwrap();

    let canvas = canvas.place_at(Point::zero());
    draw_canvas_with_background(canvas, Rgb565::new(0, 0, 0), display);
}

fn rgb888_to_rgb565(r: u8, g: u8, b: u8) -> Rgb565 {
    let [r, g, b] = rgb565::Rgb565::from_rgb888_components(r, g, b).to_rgb565_components();
    Rgb565::new(r, g, b)
}

/// Draw canvas more efficiently than default `draw` impl
fn draw_canvas_with_background<D: DrawTargetExt<Color = Rgb565>>(
    canvas: CanvasAt<Rgb565>,
    background: Rgb565,
    target: &mut D,
) where
    D::Error: Debug,
{
    target
        .fill_contiguous(
            &canvas.bounding_box(),
            canvas
                .bounding_box()
                .points()
                .map(|point| canvas.get_pixel(point).unwrap_or(background)),
        )
        .unwrap();
}

fn full_katakana_to_half_katakana(full: &str) -> String {
    // TODO: optimize by making each katakana its own string so they can be static
    let katakana_half = KATAKANA_HALF.get_or_init(|| {
        "｡｢｣､･ｦｧｨｩｪｫｬｭｮｯｰｱｲｳｴｵｶｷｸｹｺｻｼｽｾｿﾀﾁﾂﾃﾄﾅﾆﾇﾈﾉﾊﾋﾌﾍﾎﾏﾐﾑﾒﾓﾔﾕﾖﾗﾘﾙﾚﾛﾜﾝﾞﾟ"
            .graphemes(true)
            .map(Box::from)
            .collect()
    });
    let katakana_full = KATAKANA_FULL.get_or_init(|| {
		"。「」、・ヲァィゥェォャュョッーアイウエオカキクケコサシスセソタチツテトナニヌネノハヒフヘホマミムメモヤユヨラリルレロワン゛゜".graphemes(true).map(Box::from).collect()
	});

    full.graphemes(true)
        .map(|grapheme| {
            if grapheme.is_ascii() {
                // Return early if ascii to avoid searching through katakana
                return grapheme;
            }

            if let Some(index) = katakana_full
                .iter()
                .enumerate()
                .find_map(|(i, kana_full)| (&**kana_full == grapheme).then(|| i))
            {
                &*katakana_half[index]
            } else {
                grapheme
            }
        })
        .fold(String::with_capacity(full.len()), |mut str, grapheme| {
            str.push_str(&grapheme);
            str
        })
}

fn point_to_index(size: Size, top_left_offset: Point, point: Point) -> Option<usize> {
    // we must account for the top_left corner of the drawing box
    if let Ok((x, y)) = <(u32, u32)>::try_from(point - top_left_offset) {
        if x < size.width && y < size.height {
            return Some((x + y * size.width) as usize);
        }
    }

    None
}
