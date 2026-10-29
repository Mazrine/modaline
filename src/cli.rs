//! Headless batch mode: `modaline -b input [options]`.

use std::path::PathBuf;

use anyhow::{Context, Result, bail};

use modaline::editor::Editor;
use modaline::io;
use modaline::sprite::{ColorMode, DitherMode};

pub fn run(args: &[String]) -> Result<()> {
    let mut input: Option<PathBuf> = None;
    let mut save_as: Option<PathBuf> = None;
    let mut sheet_path: Option<PathBuf> = None;
    let mut data_path: Option<PathBuf> = None;
    let mut gif_path: Option<PathBuf> = None;
    let mut split_layers: Option<PathBuf> = None;
    let mut split_tags: Option<PathBuf> = None;
    let mut split_slices: Option<PathBuf> = None;
    let mut split_grid: Option<PathBuf> = None;
    let mut tileset_path: Option<PathBuf> = None;
    let mut tileset_id: Option<u32> = None;
    let mut palette_path: Option<PathBuf> = None;
    let mut script_path: Option<PathBuf> = None;
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
            "--split-layers" => split_layers = Some(PathBuf::from(next(&mut i)?)),
            "--split-tags" => split_tags = Some(PathBuf::from(next(&mut i)?)),
            "--split-slices" => split_slices = Some(PathBuf::from(next(&mut i)?)),
            "--split-grid" => split_grid = Some(PathBuf::from(next(&mut i)?)),
            "--export-tileset" | "--tileset" => {
                tileset_path = Some(PathBuf::from(next(&mut i)?));
            }
            "--tileset-id" => tileset_id = Some(next(&mut i)?.parse::<u32>()?),
            "--palette" => palette_path = Some(PathBuf::from(next(&mut i)?)),
            "--script" => script_path = Some(PathBuf::from(next(&mut i)?)),
            "--filename" => cfg.filename = next(&mut i)?,
            "--ignore-empty" => cfg.ignore_empty = true,
            "--merge" => cfg.merge_duplicates = true,
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

    let input = input.context("usage: modaline -b <input> [options] (try --help)")?;
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
    if let Some(path) = script_path.as_ref() {
        let out = modaline::script::run_file(&mut editor, path).map_err(anyhow::Error::msg)?;
        if !out.is_empty() {
            println!("{out}");
        }
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

    let view = if sheet_path.is_some()
        || gif_path.is_some()
        || data_path.is_some()
        || split_layers.is_some()
        || split_tags.is_some()
        || split_slices.is_some()
        || split_grid.is_some()
        || tileset_path.is_some()
        || palette_path.is_some()
    {
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
                .unwrap_or_else(|| "modaline".into());
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

    if let Some(dir) = split_layers.as_ref() {
        let view = view.as_ref().unwrap();
        std::fs::create_dir_all(dir)?;
        let title = save_as
            .as_ref()
            .or(Some(&input))
            .and_then(|p| p.file_stem())
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "modaline".into());
        for layer in 0..view.layers.len() {
            if view.layers[layer].group {
                continue;
            }
            let mut only = view.clone();
            for (i, l) in only.layers.iter_mut().enumerate() {
                l.visible = i == layer;
            }
            let name = view.layers[layer].name.replace(['/', ' '], "_");
            let mut layer_cfg = cfg.clone();
            layer_cfg.per_layer = false;
            layer_cfg.sequence = true;
            let stem = dir.join(format!("{title}_{name}"));
            let paths = io::export_sequence(&only, &layer_cfg, &stem)?;
            println!("{} files in {}", paths.len(), stem.display());
        }
    }
    if let Some(dir) = split_tags.as_ref() {
        let view = view.as_ref().unwrap();
        std::fs::create_dir_all(dir)?;
        for tag in &view.tags {
            let mut tag_cfg = cfg.clone();
            tag_cfg.tag = Some(tag.name.clone());
            tag_cfg.frames = None;
            tag_cfg.sequence = true;
            let sub = io::configured_sprite(view, &tag_cfg)?;
            let name = tag.name.replace(['/', ' '], "_");
            let stem = dir.join(&name);
            let paths = io::export_sequence(&sub, &tag_cfg, &stem)?;
            println!("{} files in {}", paths.len(), stem.display());
        }
    }
    if let Some(dir) = split_slices.as_ref() {
        let view = view.as_ref().unwrap();
        let first = cfg.frames.map(|(a, _)| a).unwrap_or(0);
        let paths = io::export_named_slices(view, first, dir, "slices")?;
        println!("{} files in {}", paths.len(), dir.display());
    }
    if let Some(dir) = split_grid.as_ref() {
        let view = view.as_ref().unwrap();
        let first = cfg.frames.map(|(a, _)| a).unwrap_or(0);
        let paths = io::export_slices(
            view,
            first,
            dir,
            "grid",
            cfg.columns.unwrap_or(0),
            cfg.rows.unwrap_or(0),
        )?;
        println!("{} files in {}", paths.len(), dir.display());
    }
    if let Some(path) = tileset_path.as_ref() {
        let view = view.as_ref().unwrap();
        io::export_tileset(view, tileset_id, path)?;
        println!("{}", path.display());
    }
    if let Some(path) = palette_path.as_ref() {
        let view = view.as_ref().unwrap();
        modaline::palette::save(&view.palette, path)?;
        println!("{}", path.display());
    }

    if save_as.is_none()
        && sheet_path.is_none()
        && gif_path.is_none()
        && lists.is_empty()
        && split_layers.is_none()
        && split_tags.is_none()
        && split_slices.is_none()
        && split_grid.is_none()
        && tileset_path.is_none()
        && palette_path.is_none()
    {
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
        "modaline -b <input> [options]\n\
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
         \x20 --atlas modaline|hash|array  --gif-loop N  --gif-colors N  --gif-dither\n\
         \x20 --filename <tpl>  --ignore-empty  --merge\n\
         Splits:\n\
         \x20 --split-layers <dir>  --split-tags <dir>  --split-slices <dir>\n\
         \x20 --split-grid <dir> [--columns N --rows N]\n\
         \x20 --export-tileset <png> [--tileset-id ID]  --palette <gpl|hex|pal>\n\
         \x20 --script <file.lua>\n\
         Lists:\n\
         \x20 --list-layers --list-tags --list-frames --list-slices --list-tilesets --list-palette"
    );
}
