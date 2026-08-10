use image::{imageops, DynamicImage, ImageBuffer, ImageReader, Rgba};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;

static BASE_SKIN_TEMPLATE: &[u8] = include_bytes!("../../../lib/base_skin_template.png");
static MSA_WATERMARK: &[u8] = include_bytes!("../../../lib/msa_watermark.png");

const DIGIT_FONT_COMPACT: [[u8; 7]; 10] = [
    [0b111, 0b101, 0b101, 0b101, 0b101, 0b101, 0b111],
    [0b010, 0b110, 0b010, 0b010, 0b010, 0b010, 0b111],
    [0b111, 0b001, 0b001, 0b111, 0b100, 0b100, 0b111],
    [0b111, 0b001, 0b001, 0b111, 0b001, 0b001, 0b111],
    [0b001, 0b011, 0b101, 0b101, 0b111, 0b001, 0b001],
    [0b111, 0b100, 0b111, 0b001, 0b001, 0b101, 0b011],
    [0b011, 0b100, 0b111, 0b101, 0b101, 0b101, 0b011],
    [0b111, 0b001, 0b010, 0b010, 0b100, 0b100, 0b100],
    [0b111, 0b101, 0b101, 0b111, 0b101, 0b101, 0b111],
    [0b111, 0b101, 0b101, 0b111, 0b001, 0b001, 0b111],
];

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateAllOptions {
    pub input_path: String,
    pub base_skin_path: Option<String>,
    pub show_numbers: Option<bool>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneratedSkin {
    pub num: i32,
    pub path: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateAllResult {
    pub output_dir: String,
    pub skins: Vec<GeneratedSkin>,
}

fn tile_number(row: u32, col: u32) -> i32 {
    (27 - (row * 9 + col)) as i32
}

fn load_72x24(path: &str) -> Result<ImageBuffer<Rgba<u8>, Vec<u8>>, String> {
    let img = ImageReader::open(path)
        .map_err(|e| e.to_string())?
        .with_guessed_format()
        .map_err(|e| e.to_string())?
        .decode()
        .map_err(|e| e.to_string())?;
    let mut rgba = img.to_rgba8();
    if rgba.width() != 72 || rgba.height() != 24 {
        rgba = imageops::resize(&rgba, 72, 24, imageops::FilterType::Nearest);
    }
    Ok(rgba)
}

fn tile_buffers(img: &ImageBuffer<Rgba<u8>, Vec<u8>>) -> HashMap<i32, ImageBuffer<Rgba<u8>, Vec<u8>>> {
    let mut map = HashMap::new();
    for row in 0..3u32 {
        for col in 0..9u32 {
            let num = tile_number(row, col);
            let tile = imageops::crop_imm(img, col * 8, row * 8, 8, 8).to_image();
            map.insert(num, tile);
        }
    }
    map
}

fn digit_overlay(num: i32) -> ImageBuffer<Rgba<u8>, Vec<u8>> {
    let w = 64u32;
    let h = 64u32;
    let mut img = ImageBuffer::from_pixel(w, h, Rgba([0u8, 0, 0, 0]));
    let digits: Vec<u32> = num
        .to_string()
        .chars()
        .map(|c| c.to_digit(10).unwrap_or(0))
        .collect();

    let underline: u8 = 0b111;
    let digit_w = 3i32;
    let bit_w = 3i32;
    let gap = 1i32;
    let total_w = digits.len() as i32 * digit_w + (digits.len() as i32 - 1) * gap;
    let torso_x = 20i32;
    let torso_w = 8i32;
    let start_x = torso_x + ((torso_w - total_w) / 2);
    let start_y = 22i32;

    for (d, digit) in digits.iter().enumerate() {
        let glyph = DIGIT_FONT_COMPACT[*digit as usize];
        let ox = start_x + d as i32 * (digit_w + gap);
        for row in 0..7i32 {
            let bits = if row == 6 { underline } else { glyph[row as usize] };
            for col in 0..digit_w {
                let bit = (bits >> (bit_w - 1 - col)) & 1;
                if bit != 0 {
                    let px = ox + col;
                    let py = start_y + row;
                    if px >= 0 && py >= 0 && px < w as i32 && py < h as i32 {
                        img.put_pixel(px as u32, py as u32, Rgba([255, 255, 255, 255]));
                    }
                }
            }
        }
    }
    img
}

fn overlay_resized(source: &[u8], width: u32, height: u32) -> Result<ImageBuffer<Rgba<u8>, Vec<u8>>, String> {
    let img = image::load_from_memory(source)
        .map_err(|e| e.to_string())?
        .to_rgba8();
    Ok(imageops::resize(&img, width, height, imageops::FilterType::Nearest))
}

fn encode_png(img: &ImageBuffer<Rgba<u8>, Vec<u8>>) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    DynamicImage::ImageRgba8(img.clone())
        .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(out)
}

fn generate_skin(
    num: i32,
    tile: &ImageBuffer<Rgba<u8>, Vec<u8>>,
    base_skin: Option<&[u8]>,
    show_numbers: bool,
) -> Result<Vec<u8>, String> {
    let mut base = match base_skin {
        Some(bytes) => overlay_resized(bytes, 64, 64)?,
        None => overlay_resized(BASE_SKIN_TEMPLATE, 64, 64)?,
    };

    // clear the regions that are re-drawn by the art tile
    for (left, top) in [(40u32, 0u32), (40, 8), (8, 32)] {
        for y in 0..8u32 {
            for x in 0..8u32 {
                base.put_pixel(left + x, top + y, Rgba([0, 0, 0, 0]));
            }
        }
    }

    imageops::overlay(&mut base, tile, 8, 8);
    if show_numbers {
        let digits = digit_overlay(num);
        imageops::overlay(&mut base, &digits, 0, 0);
    }
    let msa = overlay_resized(MSA_WATERMARK, 64, 64)?;
    imageops::overlay(&mut base, &msa, 0, 0);

    encode_png(&base)
}

#[tauri::command]
pub async fn generate_all(opts: GenerateAllOptions) -> Result<GenerateAllResult, String> {
    let art = load_72x24(&opts.input_path)?;
    let tiles = tile_buffers(&art);

    let output_dir = std::env::temp_dir().join(format!(
        "skinartplus_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    ));
    fs::create_dir_all(&output_dir).map_err(|e| e.to_string())?;

    let base_skin_bytes: Option<Vec<u8>> = match &opts.base_skin_path {
        Some(p) => Some(fs::read(p).map_err(|e| e.to_string())?),
        None => None,
    };
    let show_numbers = opts.show_numbers.unwrap_or(true);

    let mut skins = Vec::new();
    for num in 1..=26 {
        let tile = tiles
            .get(&num)
            .ok_or_else(|| format!("missing tile {}", num))?;
        let png = generate_skin(num, tile, base_skin_bytes.as_deref(), show_numbers)?;
        let out_path = output_dir.join(format!("skin_{}.png", num));
        fs::write(&out_path, png).map_err(|e| e.to_string())?;
        skins.push(GeneratedSkin {
            num,
            path: out_path.to_string_lossy().to_string(),
        });
    }

    Ok(GenerateAllResult {
        output_dir: output_dir.to_string_lossy().to_string(),
        skins,
    })
}

#[cfg(test)]
mod debug_generate {
    use super::*;

    #[test]
    fn debug_generate() {
        let path = r"C:\Users\global pc\rust\SkinartPlus\templates\images\iraq.png";
        let art = load_72x24(path).expect("load");
        let tiles = tile_buffers(&art);
        println!("tiles={}", tiles.len());
        for num in 1..=26 {
            match generate_skin(num, tiles.get(&num).unwrap(), None, true) {
                Ok(png) => println!("num={} ok len={}", num, png.len()),
                Err(e) => {
                    println!("ERR num={} : {}", num, e);
                    return;
                }
            }
        }
        println!("ALL OK");
    }
}
