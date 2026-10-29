//! Headless batch mode: `vixel -b input [options]`.

use std::path::PathBuf;

use anyhow::{Context, Result, bail};

use vixel::editor::Editor;
use vixel::io;
use vixel::sprite::{ColorMode, DitherMode};

pub fn run(args: &[String]) -> Result<()> {
    let mut input: Option<PathBuf> = None;
    let mut save_as: Option<PathBuf> = None;
    let mut sheet_path: Option<PathBuf> = None;
    let mut data_path: Option<PathBuf> = None;
    let mut gif_path: Option<PathBuf> = None;
    let mut cfg = io::ExportConfig {
        gif: false,
        meta: false,
        sheet: false,
        ..Default::default()
    };
    let mut crop: Option<(u32, u32, u32, u32)> = None;
    let mut trim = false;
    let mut color_mode: Option<ColorMode> = None;
    let mut dither: Option<DitherMode> = None;
    let mut lists: Vec<String> = Vec::new();

    let mut i = 1; // skip argv[0]
    while i < args.len() {
        let arg = args[i].as_str();
        let next = |i: &mut usize| -> Result<String> {
            *i += 1;
            args.get(*i)
                .cloned()
                .with_context(|| format!("{arg} needs a value"))
        };
        match arg {
            "-b" | "--batch" => {}
            "--save-as" => save_as = Some(PathBuf::from(next(&mut i)?)),
            "--sheet" => {
                sheet_path = Some(PathBuf::from(next(&mut i)?));
                cfg.sheet = true;
            }
            "--data" => {
                data_path = Some(PathBuf::from(next(&mut i)?));
                cfg.meta = true;
            }
            "--gif" => {
                gif_path = Some(PathBuf::from(next(&mut i)?));
                cfg.gif = true;
            }
            "--scale" => cfg.scale = next(&mut i)?.parse::<u32>()?.clamp(1, 16),
            "--color-mode" | "--color" => {
                let v = next(&mut i)?;
                color_mode = ColorMode::parse(&v);
                if color_mode.is_none() {
                    bail!("unknown colour mode {v}");
                }
            }
            "--dither" => {
                let v = next(&mut i)?;
                dither = DitherMode::parse(&v);
                if dither.is_none() {
                    bail!("unknown dither mode {v}");
                }
            }
            "--frame-range" => {
                let v = next(&mut i)?;
                let (a, b) = if let Some((a, b)) = v.split_once('-') {
                    (a.parse::<usize>()?, b.parse::<usize>()?)
                } else {
                    let b = next(&mut i)?.parse::<usize>()?;
                    (v.parse::<usize>()?, b)
                };
                if a == 0 || b == 0 {
                    bail!("frame ranges are 1-based");
                }
                cfg.frames = Some((a - 1, b - 1));
            }
            "--tag" => cfg.tag = Some(next(&mut i)?),
            "--layers" => {
                let v = next(&mut i)?;
                cfg.layers = match v.as_str() {
                    "all" => io::ExportLayers::All,
                    "visible" => io::ExportLayers::Visible,
                    "indices" => io::ExportLayers::Indices,
                    _ => bail!("--layers wants all|visible|indices"),
                };
            }
            "--layer-list" => {
                cfg.layer_indices = next(&mut i)?
                    .split(',')
                    .filter_map(|v| v.parse::<usize>().ok())
                    .map(|v| v.saturating_sub(1))
                    .collect();
                cfg.layers = io::ExportLayers::Indices;
            }
            "--trim" => trim = true,
            "--crop" => {
                let x = next(&mut i)?.parse::<u32>()?;
                let y = next(&mut i)?.parse::<u32>()?;
                let w = next(&mut i)?.parse::<u32>()?;
                let h = next(&mut i)?.parse::<u32>()?;
                crop = Some((x, y, w, h));
            }
            "--layout" => {
                let v = next(&mut i)?;
                cfg.sheet_layout = Some(io::SheetLayout::parse(&v).context("unknown layout")?);
            }
            "--columns" => cfg.columns = Some(next(&mut i)?.parse::<u32>()?.max(1)),
            "--rows" => cfg.rows = Some(next(&mut i)?.parse::<u32>()?.max(1)),
            "--border" => cfg.border_padding = next(&mut i)?.parse::<u32>()?.min(64),
            "--shape" => cfg.shape_padding = next(&mut i)?.parse::<u32>()?.min(64),
            "--inner" => cfg.inner_padding = next(&mut i)?.parse::<u32>()?.min(64),
            "--extrude" => cfg.extrude = next(&mut i)?.parse::<u32>()?.min(64),
            "--padding" => cfg.padding = next(&mut i)?.parse::<u32>()?.min(64),
            "--atlas" => {
                let v = next(&mut i)?;
                cfg.atlas = io::AtlasFormat::parse(&v).context("unknown atlas format")?;
            }
            "--gif-loop" => cfg.gif_loop = next(&mut i)?.parse::<u16>()?,
            "--gif-colors" => cfg.gif_colors = next(&mut i)?.parse::<u32>()?.clamp(2, 256) as u16,
            "--gif-dither" => cfg.gif_dither = true,
            "--list-layers" | "--list-tags" | "--list-frames" | "--list-slices"
            | "--list-tilesets" | "--list-palette" => lists.push(arg.to_string()),
            "--help" | "-h" => {
                print_help();
                return Ok(());
            }
            other if other.starts_with('-') => bail!("unknown option {other}"),
            other => {
                if input.is_some() {
                    bail!("unexpected argument {other}");
                }
                input = Some(PathBuf::from(other));
            }
        }
        i += 1;
    }

    let input = input.context("usage: vixel -b <input> [options] (try --help)")?;
    let mut editor = Editor::new(1, 1);
    editor.replace_sprite(io::load_any(&input)?);
    if let Some((x, y, w, h)) = crop {
        editor.crop_to(x, y, w, h);
    }
    if trim {
        editor.trim();
    }
    if let Some(mode) = dither {
        editor.dither = mode;
    }
    if let Some(mode) = color_mode {
        editor.set_color_mode(mode);
    }
    let sprite = editor.sprite.clone();

    for list in &lists {
        match list.as_str() {
            "--list-layers" => {
                for (i, l) in sprite.layers.iter().enumerate() {
                    println!(
                        "{i}\t{}\tvisible={}\teditable={}\ttilemap={}",
                        l.name, l.visible, l.editable, l.tilemap
                    );
                }
            }
            "--list-tags" => {
                for t in &sprite.tags {
                    println!(
                        "{}\t{}..{}\t{}\trepeat={}",
                        t.name,
                        t.from,
                        t.to,
                        t.direction.name(),
                        t.repeat
                    );
                }
            }
            "--list-frames" => {
                for (i, f) in sprite.frames.iter().enumerate() {
                    println!("{i}\t{}ms", f.duration_ms);
                }
            }
            "--list-slices" => {
                for s in &sprite.slices {
                    println!("{}\t{} keys", s.name, s.keys.len());
                }
            }
            "--list-tilesets" => {
                for t in &sprite.tilesets {
                    println!(
                        "{}\t{}\t{}x{}/{} tiles\tbase={}",
                        t.id, t.name, t.tile_w, t.tile_h, t.num_tiles, t.base_index
                    );
                }
            }
            "--list-palette" => {
                for (i, c) in sprite.palette.iter().enumerate() {
                    println!("{i}\t#{:02x}{:02x}{:02x}{:02x}", c.0, c.1, c.2, c.3);
                }
            }
            _ => {}
        }
    }

    if let Some(path) = save_as.as_ref() {
        editor.save_path(path)?;
        println!("{}", path.display());
    }

    let view = if sheet_path.is_some() || gif_path.is_some() || data_path.is_some() {
        Some(io::configured_sprite(&editor.sprite, &cfg)?)
    } else {
        None
    };

    if let Some(path) = sheet_path.as_ref() {
        let view = view.as_ref().unwrap();
        let sheet = if cfg.sheet_layout.is_some() {
            io::export_sheet_layout(view, path, &cfg)?
        } else {
            io::export_sheet_scaled_padded(view, path, None, cfg.scale, cfg.padding)?
        };
        println!("{}", path.display());
        if let Some(data) = data_path.as_ref() {
            let size = image::open(path)?.to_rgba8().dimensions();
            let image_name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let title = save_as
                .as_ref()
                .or(Some(&input))
                .and_then(|p| p.file_stem())
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "vixel".into());
            io::export_meta_format(view, &sheet, &image_name, size, data, &cfg, &title)?;
            println!("{}", data.display());
        }
    } else if data_path.is_some() {
        bail!("--data needs --sheet");
    }

    if let Some(path) = gif_path.as_ref() {
        io::export_gif_configured(
            view.as_ref().unwrap(),
            path,
            cfg.scale,
            cfg.gif_loop,
            cfg.gif_dither,
            cfg.gif_colors,
        )?;
        println!("{}", path.display());
    }

    if save_as.is_none() && sheet_path.is_none() && gif_path.is_none() && lists.is_empty() {
        println!(
            "{}x{}  {} layer(s)  {} frame(s)",
            sprite.width,
            sprite.height,
            sprite.layers.len(),
            sprite.frames.len()
        );
    }
    Ok(())
}

fn print_help() {
    println!(
        "vixel -b <input> [options]\n\
         \n\
         Outputs:\n\
         \x20 --save-as <path>       save transformed sprite (.ase/image/.vpx)\n\
         \x20 --sheet <path>         export sprite sheet\n\
         \x20 --data <path>          atlas/meta JSON beside --sheet\n\
         \x20 --gif <path>           export GIF\n\
         Options:\n\
         \x20 --scale N  --frame-range A-B  --tag NAME  --layers all|visible|indices\n\
         \x20 --layer-list 1,2  --trim  --crop X Y W H\n\
         \x20 --color-mode auto|rgba|grayscale|indexed  --dither none|bayer2|bayer4|bayer8|floyd\n\
         \x20 --layout h|v|rows|columns|packed  --columns N  --rows N\n\
         \x20 --border N  --shape N  --inner N  --extrude N  --padding N\n\
         \x20 --atlas vixel|hash|array  --gif-loop N  --gif-colors N  --gif-dither\n\
         Lists:\n\
         \x20 --list-layers --list-tags --list-frames --list-slices --list-tilesets --list-palette"
    );
}
