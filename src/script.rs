//! Aseprite-style Lua scripting (MVP).
//!
//! The engine is sandboxed: no `io`/`os`/`package`/`debug`, a 64 MiB memory
//! cap and a 50M-instruction budget. The exposed `app` table covers the core
//! sprite/layer/pixel API; P5-50 grows it toward the full Aseprite surface.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use mlua::{HookTriggers, Lua, LuaOptions, StdLib, Table, Value, VmState};

use crate::color::Rgba;
use crate::editor::Editor;

struct Host<'a> {
    editor: &'a mut Editor,
    out: Vec<String>,
}

type HostRef<'a> = Rc<RefCell<Host<'a>>>;

/// Run `source` and return its print/alert output. Any sprite changes are one
/// snapshot undo step.
pub fn run(editor: &mut Editor, source: &str, name: &str) -> Result<String, String> {
    let before = editor.sprite.clone();
    let before_revision = editor.revision;
    let (result, output) = {
        let host: HostRef = Rc::new(RefCell::new(Host {
            editor,
            out: Vec::new(),
        }));
        let result = execute(host.clone(), source, name);
        let output = host.borrow().out.join("\n");
        (result, output)
    };
    if editor.revision != before_revision {
        editor.commit_script(before, before_revision);
        editor.msg = if output.is_empty() {
            "script finished".into()
        } else {
            output
                .lines()
                .last()
                .unwrap_or("script finished")
                .to_string()
        };
    }
    match result {
        Ok(()) => Ok(output),
        Err(e) => Err(format!("{name}: {e}")),
    }
}

pub fn run_file(editor: &mut Editor, path: &Path) -> Result<String, String> {
    let source =
        std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    run(editor, &source, &path.display().to_string())
}

fn execute(host: HostRef<'_>, source: &str, name: &str) -> Result<(), mlua::Error> {
    let libs = StdLib::TABLE | StdLib::STRING | StdLib::MATH | StdLib::UTF8;
    let lua = Lua::new_with(libs, LuaOptions::default())?;
    lua.set_memory_limit(64 * 1024 * 1024)?;
    let ticks = Arc::new(AtomicUsize::new(0));
    let budget = ticks.clone();
    lua.set_hook(
        HookTriggers::new().every_nth_instruction(1_000_000),
        move |_, _| {
            if budget.fetch_add(1, Ordering::Relaxed) >= 50 {
                Err(mlua::Error::RuntimeError(
                    "script instruction budget exceeded".into(),
                ))
            } else {
                Ok(VmState::Continue)
            }
        },
    )?;
    for blocked in [
        "io", "os", "package", "debug", "dofile", "loadfile", "require",
    ] {
        lua.globals().set(blocked, Value::Nil)?;
    }

    lua.scope(|scope| {
        let app = lua.create_table()?;
        app.set("version", env!("CARGO_PKG_VERSION"))?;
        app.set("activeFrame", host.borrow().editor.active_frame as i64 + 1)?;

        let alert_host = host.clone();
        let alert = scope.create_function_mut(move |_, msg: String| {
            alert_host.borrow_mut().out.push(msg);
            Ok(())
        })?;
        app.set("alert", alert)?;

        let print_host = host.clone();
        let print = scope.create_function_mut(move |_, args: mlua::MultiValue| {
            let text = args
                .iter()
                .map(|v| match v {
                    Value::String(s) => s.to_string_lossy().to_string(),
                    Value::Integer(i) => i.to_string(),
                    Value::Number(n) => n.to_string(),
                    Value::Boolean(b) => b.to_string(),
                    Value::Nil => "nil".into(),
                    other => format!("{other:?}"),
                })
                .collect::<Vec<_>>()
                .join("\t");
            print_host.borrow_mut().out.push(text);
            Ok(())
        })?;
        lua.globals().set("print", print)?;

        let sprite = lua.create_table()?;
        let sprite_meta = lua.create_table()?;
        let index_host = host.clone();
        sprite_meta.set(
            "__index",
            scope.create_function(move |_, (_t, key): (Table, String)| {
                let h = index_host.borrow();
                let e = &h.editor;
                Ok(match key.as_str() {
                    "width" => Value::Integer(e.sprite.width as i64),
                    "height" => Value::Integer(e.sprite.height as i64),
                    "frameCount" => Value::Integer(e.sprite.frames.len() as i64),
                    "layerCount" => Value::Integer(e.sprite.layers.len() as i64),
                    "activeLayer" => Value::Integer(e.active_layer as i64 + 1),
                    "activeFrame" => Value::Integer(e.active_frame as i64 + 1),
                    _ => Value::Nil,
                })
            })?,
        )?;
        sprite_meta.set(
            "__newindex",
            scope.create_function(|_, (_t, key, _): (Table, String, Value)| {
                Err::<(), _>(mlua::Error::RuntimeError(format!(
                    "sprite.{key} is read-only"
                )))
            })?,
        )?;
        let resize_host = host.clone();
        sprite.set(
            "resize",
            scope.create_function_mut(move |_, (_t, w, h): (Table, u32, u32)| {
                if w == 0 || h == 0 {
                    return Err(mlua::Error::RuntimeError(
                        "resize dimensions must be positive".into(),
                    ));
                }
                let mut host = resize_host.borrow_mut();
                host.editor.sprite.resize(w, h);
                host.editor.cursor = (
                    host.editor.cursor.0.min(w - 1),
                    host.editor.cursor.1.min(h - 1),
                );
                host.editor.touch();
                host.editor.dirty = true;
                Ok(())
            })?,
        )?;

        let get_host = host.clone();
        sprite.set(
            "getPixel",
            scope.create_function_mut(move |lua, (_t, x, y): (Table, u32, u32)| {
                let host = get_host.borrow();
                let c = host.editor.sprite.get(
                    host.editor.active_layer,
                    host.editor.active_frame,
                    x,
                    y,
                );
                color_table(lua, c)
            })?,
        )?;

        let set_host = host.clone();
        sprite.set(
            "setPixel",
            scope.create_function_mut(move |_, (_t, x, y, color): (Table, u32, u32, Value)| {
                let c = parse_color(color)?;
                let mut host = set_host.borrow_mut();
                host.editor.script_set_pixel(x, y, c);
                Ok(())
            })?,
        )?;

        let layer_new_host = host.clone();
        sprite.set(
            "newLayer",
            scope.create_function_mut(move |_, _t: Table| {
                Ok(layer_new_host.borrow_mut().editor.script_add_layer() + 1)
            })?,
        )?;
        let frame_new_host = host.clone();
        sprite.set(
            "newFrame",
            scope.create_function_mut(move |_, _t: Table| {
                Ok(frame_new_host.borrow_mut().editor.script_add_frame() + 1)
            })?,
        )?;

        let save_as_host = host.clone();
        sprite.set(
            "saveAs",
            scope.create_function_mut(move |_, (_t, path): (Table, String)| {
                save_as_host
                    .borrow_mut()
                    .editor
                    .save_path(Path::new(&path))
                    .map_err(|e| mlua::Error::RuntimeError(e.to_string()))
            })?,
        )?;
        let save_host = host.clone();
        sprite.set(
            "save",
            scope.create_function_mut(move |_, _t: Table| {
                let mut host = save_host.borrow_mut();
                let path =
                    host.editor.path.clone().ok_or_else(|| {
                        mlua::Error::RuntimeError("sprite has no filename".into())
                    })?;
                host.editor
                    .save_path(&path)
                    .map_err(|e| mlua::Error::RuntimeError(e.to_string()))
            })?,
        )?;

        sprite.set_metatable(Some(sprite_meta))?;
        app.set("activeSprite", sprite)?;

        let layer = lua.create_table()?;
        let layer_meta = lua.create_table()?;
        let idx = host.borrow().editor.active_layer;
        let layer_index_host = host.clone();
        layer_meta.set(
            "__index",
            scope.create_function(move |lua, (_t, key): (Table, String)| {
                let h = layer_index_host.borrow();
                let Some(l) = h.editor.sprite.layers.get(idx) else {
                    return Ok(Value::Nil);
                };
                Ok(match key.as_str() {
                    "name" => Value::String(lua.create_string(&l.name)?),
                    "visible" => Value::Boolean(l.visible),
                    "opacity" => Value::Integer(l.opacity as i64),
                    "editable" => Value::Boolean(l.editable),
                    "isBackground" => Value::Boolean(l.background),
                    "isReference" => Value::Boolean(l.reference),
                    _ => Value::Nil,
                })
            })?,
        )?;
        let layer_set_host = host.clone();
        layer_meta.set(
            "__newindex",
            scope.create_function_mut(move |_, (_t, key, value): (Table, String, Value)| {
                let mut h = layer_set_host.borrow_mut();
                let Some(l) = h.editor.sprite.layers.get_mut(idx) else {
                    return Err(mlua::Error::RuntimeError("layer disappeared".into()));
                };
                match key.as_str() {
                    "name" => {
                        l.name = value
                            .as_string()
                            .map(|s| s.to_string_lossy().to_string())
                            .ok_or_else(|| {
                                mlua::Error::RuntimeError("layer.name must be a string".into())
                            })?
                    }
                    "visible" => {
                        l.visible = value.as_boolean().ok_or_else(|| {
                            mlua::Error::RuntimeError("layer.visible must be boolean".into())
                        })?
                    }
                    "editable" => {
                        l.editable = value.as_boolean().ok_or_else(|| {
                            mlua::Error::RuntimeError("layer.editable must be boolean".into())
                        })?
                    }
                    "opacity" => {
                        l.opacity = value
                            .as_integer()
                            .ok_or_else(|| {
                                mlua::Error::RuntimeError("layer.opacity must be a number".into())
                            })?
                            .clamp(0, 255) as u8
                    }
                    other => {
                        return Err(mlua::Error::RuntimeError(format!(
                            "layer.{other} is read-only"
                        )));
                    }
                }
                h.editor.touch();
                h.editor.dirty = true;
                Ok(())
            })?,
        )?;
        layer.set_metatable(Some(layer_meta))?;
        app.set("activeLayer", layer)?;

        lua.globals().set("app", app)?;
        lua.load(source).set_name(name).exec()?;
        Ok(())
    })
}

fn color_table(lua: &Lua, c: Rgba) -> Result<Table, mlua::Error> {
    let t = lua.create_table()?;
    t.set("r", c.0)?;
    t.set("g", c.1)?;
    t.set("b", c.2)?;
    t.set("a", c.3)?;
    Ok(t)
}

fn parse_color(value: Value) -> Result<Rgba, mlua::Error> {
    match value {
        Value::Nil => Ok(Rgba::TRANSPARENT),
        Value::String(s) => crate::color::parse_hex(&s.to_string_lossy())
            .ok_or_else(|| mlua::Error::RuntimeError("color string must be #rrggbb".into())),
        Value::Table(t) => {
            let channel = |name: &str| -> Result<u8, mlua::Error> {
                Ok(t.get::<Option<i64>>(name)?.unwrap_or(0).clamp(0, 255) as u8)
            };
            let a = t.get::<Option<i64>>("a")?;
            Ok(Rgba(
                channel("r")?,
                channel("g")?,
                channel("b")?,
                a.map(|v| v.clamp(0, 255) as u8).unwrap_or(255),
            ))
        }
        _ => Err(mlua::Error::RuntimeError(
            "color must be a table {r,g,b,a}, #rrggbb, or nil".into(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripts_read_and_write_pixels_as_one_undo_step() {
        let mut e = Editor::new(4, 4);
        let out = run(
            &mut e,
            r#"
            local s = app.activeSprite
            s:setPixel(1, 2, {r=200, g=10, b=20, a=255})
            local c = s:getPixel(1, 2)
            assert(c.r == 200 and c.g == 10 and c.b == 20 and c.a == 255)
            print("pixel ok", s.width, s.height)
            "#,
            "test.lua",
        )
        .unwrap();
        assert!(out.contains("pixel ok"));
        assert_eq!(e.sprite.get(0, 0, 1, 2), Rgba::rgb(200, 10, 20));
        e.undo();
        assert_eq!(e.sprite.get(0, 0, 1, 2), Rgba::TRANSPARENT);
    }

    #[test]
    fn layer_properties_are_live_and_resize_works() {
        let mut e = Editor::new(4, 4);
        run(
            &mut e,
            r#"
            local l = app.activeLayer
            l.name = "paint"
            l.visible = false
            l.opacity = 128
            assert(app.activeLayer.name == "paint")
            app.activeSprite:resize(8, 6)
            "#,
            "layer.lua",
        )
        .unwrap();
        assert_eq!(e.sprite.layers[0].name, "paint");
        assert!(!e.sprite.layers[0].visible);
        assert_eq!(e.sprite.layers[0].opacity, 128);
        assert_eq!((e.sprite.width, e.sprite.height), (8, 6));
    }

    #[test]
    fn sandbox_hides_file_and_os_access() {
        let mut e = Editor::new(2, 2);
        run(
            &mut e,
            "assert(io == nil); assert(os == nil); assert(package == nil); assert(dofile == nil); assert(debug == nil)",
            "sandbox.lua",
        )
        .unwrap();
    }

    #[test]
    fn save_as_and_add_layer_frame_are_available() {
        let mut e = Editor::new(2, 2);
        run(
            &mut e,
            "assert(app.activeSprite:newLayer() == 2); assert(app.activeSprite:newFrame() == 2)",
            "structure.lua",
        )
        .unwrap();
        assert_eq!(e.sprite.layers.len(), 2);
        assert_eq!(e.sprite.frames.len(), 2);
    }

    #[test]
    fn errors_carry_the_script_name() {
        let mut e = Editor::new(2, 2);
        let err = run(&mut e, "error('boom')", "bad.lua").unwrap_err();
        assert!(err.contains("bad.lua"), "{err}");
        assert!(err.contains("boom"), "{err}");
    }
}
