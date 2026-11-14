//! Native macOS menu bar, built from the workspace layout with `muda`.

use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender, channel};

use muda::accelerator::Accelerator;
use muda::{AboutMetadata, CheckMenuItem, Menu, MenuEvent, MenuItem as NativeItem, PredefinedMenuItem, Submenu};

use crate::commands::{Command, CommandItem, FIELD_SAFE_IDS, Layout, MenuItem, MenuRole};

enum Handle {
    Normal(NativeItem),
    Check(CheckMenuItem),
}

pub struct NativeMenu {
    _menu: Menu,
    live: Vec<(Command, Handle)>,
    /// Last state pushed to each item, so we only call into AppKit on change.
    shown: HashMap<Command, (bool, Option<bool>)>,
    events: Receiver<String>,
}

impl NativeMenu {
    /// Must be called after the app has finished launching (inside the eframe app creator).
    pub fn install(layout: &Layout, ctx: egui::Context) -> Result<Self, muda::Error> {
        let (tx, rx): (Sender<String>, Receiver<String>) = channel();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            let _ = tx.send(event.id.0);
            ctx.request_repaint();
        }));

        let menu = Menu::new();
        let app_menu = Submenu::new("Vector Studio", true);
        app_menu.append_items(&[
            &PredefinedMenuItem::about(
                Some("About Vector Studio"),
                Some(AboutMetadata {
                    name: Some("Vector Studio".into()),
                    version: Some(env!("CARGO_PKG_VERSION").into()),
                    ..Default::default()
                }),
            ),
            &PredefinedMenuItem::separator(),
            &NativeItem::with_id("app.settings", "Settings…", false, "CmdOrCtrl+,".parse().ok()),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::services(None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::hide(None),
            &PredefinedMenuItem::hide_others(None),
            &PredefinedMenuItem::show_all(None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::quit(None),
        ])?;
        menu.append(&app_menu)?;

        let mut live = Vec::new();
        for m in &layout.menus {
            let sub = Submenu::new(&m.label, true);
            append_items(&sub, &m.items, &mut live)?;
            menu.append(&sub)?;
            match m.role {
                MenuRole::Window => sub.set_as_windows_menu_for_nsapp(),
                MenuRole::Help => sub.set_as_help_menu_for_nsapp(),
                MenuRole::None => {}
            }
        }
        menu.init_for_nsapp();

        Ok(Self { _menu: menu, live, shown: HashMap::new(), events: rx })
    }

    /// Commands chosen from the menu since the last call.
    pub fn drain(&self) -> Vec<Command> {
        self.events.try_iter().filter_map(|id| Command::from_id(&id)).collect()
    }

    /// Push enabled/checked state for live items when it changed.
    pub fn sync(&mut self, state: impl Fn(Command) -> (bool, Option<bool>)) {
        for (cmd, handle) in &self.live {
            let now = state(*cmd);
            if self.shown.get(cmd) == Some(&now) {
                continue;
            }
            match handle {
                Handle::Normal(item) => item.set_enabled(now.0),
                Handle::Check(item) => {
                    item.set_enabled(now.0);
                    item.set_checked(now.1.unwrap_or(false));
                }
            }
            self.shown.insert(*cmd, now);
        }
    }
}

fn append_items(parent: &Submenu, items: &[MenuItem], live: &mut Vec<(Command, Handle)>) -> Result<(), muda::Error> {
    for item in items {
        match item {
            MenuItem::Separator(_) => parent.append(&PredefinedMenuItem::separator())?,
            MenuItem::Submenu { label, items } => {
                let sub = Submenu::new(label, !items.is_empty());
                append_items(&sub, items, live)?;
                parent.append(&sub)?;
            }
            MenuItem::Command(c) => {
                let command = Command::from_id(&c.cmd);
                let accel = accelerator(c);
                if c.checkable {
                    let item = CheckMenuItem::with_id(&c.cmd, &c.label, command.is_some(), false, accel);
                    parent.append(&item)?;
                    if let Some(cmd) = command {
                        live.push((cmd, Handle::Check(item)));
                    }
                } else {
                    let item = NativeItem::with_id(&c.cmd, &c.label, command.is_some(), accel);
                    parent.append(&item)?;
                    if let Some(cmd) = command {
                        live.push((cmd, Handle::Normal(item)));
                    }
                }
            }
        }
    }
    Ok(())
}

/// Shortcuts left to the canvas key handler, which skips them while a text field has focus:
/// bare letters (e.g. "L" for Align Left) and the clipboard/undo keys. Cmd+Q stays Quit,
/// as on every Mac app.
fn accelerator(item: &CommandItem) -> Option<Accelerator> {
    let text = item.shortcut.as_deref()?;
    if text.len() == 1 || FIELD_SAFE_IDS.contains(&item.cmd.as_str()) || text == "CmdOrCtrl+Q" {
        return None;
    }
    match text.parse() {
        Ok(a) => Some(a),
        Err(err) => {
            log::warn!("menu shortcut {text:?} on {} not understood: {err}", item.cmd);
            None
        }
    }
}
