// ICQ-style desktop client. Contact list window + one native window per chat.
use eframe::egui::{self, Color32, RichText, ViewportBuilder, ViewportId};

const GREEN: Color32 = Color32::from_rgb(0x3c, 0xb0, 0x3c);
const RED: Color32 = Color32::from_rgb(0xd0, 0x30, 0x30);

#[derive(Clone, Copy, PartialEq)]
enum Status { Online, Away, Na, Occupied, Dnd, FreeForChat, Invisible, Offline }

impl Status {
    const ALL: [Status; 8] = [Status::Online, Status::Away, Status::Na, Status::Occupied,
        Status::Dnd, Status::FreeForChat, Status::Invisible, Status::Offline];
    fn label(self) -> &'static str {
        match self {
            Status::Online => "Online", Status::Away => "Away", Status::Na => "N/A",
            Status::Occupied => "Occupied", Status::Dnd => "DND",
            Status::FreeForChat => "Free For Chat", Status::Invisible => "Invisible",
            Status::Offline => "Offline",
        }
    }
    fn color(self) -> Color32 {
        match self {
            Status::Offline | Status::Dnd | Status::Occupied => RED,
            Status::Away | Status::Na => Color32::from_rgb(0xe0, 0xa0, 0x20),
            Status::Invisible => Color32::GRAY,
            _ => GREEN,
        }
    }
}

// The ICQ flower: 8 petals around a center, coloured by status.
fn flower(ui: &mut egui::Ui, status: Status, size: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    let c = rect.center();
    let p = ui.painter();
    for i in 0..8 {
        let a = i as f32 * std::f32::consts::TAU / 8.0;
        let col = if i == 0 && status != Status::Offline { RED } else { status.color() };
        p.circle_filled(c + egui::vec2(a.cos(), a.sin()) * size * 0.3, size * 0.17, col);
    }
    p.circle_filled(c, size * 0.15, Color32::YELLOW);
}

struct Contact { uin: u32, nick: String, status: Status, history: Vec<(bool, String)>, draft: String, open: bool, unread: bool }

struct App { me: Status, contacts: Vec<Contact> }

fn uh_oh() {
    // ponytail: system sound via afplay; bundle the real "uh-oh" wav when you have one.
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("afplay").arg("/System/Library/Sounds/Pop.aiff").spawn();
}

impl App {
    fn new() -> Self {
        let c = |uin, nick: &str, status| Contact { uin, nick: nick.into(), status, history: vec![], draft: String::new(), open: false, unread: false };
        App { me: Status::Online, contacts: vec![
            c(123456, "Echo Bot", Status::Online), c(281337, "Alice", Status::Away),
            c(555123, "Bob", Status::FreeForChat), c(900001, "Carol", Status::Offline),
        ]}
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                flower(ui, self.me, 18.0);
                egui::ComboBox::from_id_salt("me").selected_text(self.me.label()).show_ui(ui, |ui| {
                    for s in Status::ALL { ui.selectable_value(&mut self.me, s, s.label()); }
                });
            });
        });
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.label(RichText::new("ICQ").strong().size(20.0).color(GREEN));
            ui.separator();
            for (title, online) in [("Online", true), ("Offline", false)] {
                egui::CollapsingHeader::new(RichText::new(title).strong()).default_open(true).show(ui, |ui| {
                    for ct in self.contacts.iter_mut().filter(|c| (c.status != Status::Offline) == online) {
                        ui.horizontal(|ui| {
                            flower(ui, ct.status, 14.0);
                            let text = if ct.unread { RichText::new(&ct.nick).strong().color(RED) } else { RichText::new(&ct.nick) };
                            if ui.add(egui::Label::new(text).sense(egui::Sense::click())).double_clicked() {
                                ct.open = true;
                                ct.unread = false;
                            }
                        });
                    }
                });
            }
        });

        for ct in self.contacts.iter_mut().filter(|c| c.open) {
            let title = format!("{} ({}) - Message Session", ct.nick, ct.uin);
            ctx.show_viewport_immediate(ViewportId::from_hash_of(ct.uin),
                ViewportBuilder::default().with_title(title).with_inner_size([360.0, 320.0]), |ctx, _| {
                egui::TopBottomPanel::bottom("send").show(ctx, |ui| {
                    ui.add(egui::TextEdit::multiline(&mut ct.draft).desired_rows(3).desired_width(f32::INFINITY));
                    let enter = ui.input(|i| i.key_pressed(egui::Key::Enter) && i.modifiers.command);
                    if (ui.button("Send").clicked() || enter) && !ct.draft.trim().is_empty() {
                        let msg = std::mem::take(&mut ct.draft).trim().to_string();
                        ct.history.push((true, msg.clone()));
                        // ponytail: no network; the Echo Bot replies locally. Plug a real protocol in here.
                        if ct.uin == 123456 { ct.history.push((false, msg)); uh_oh(); }
                    }
                });
                egui::CentralPanel::default().show(ctx, |ui| {
                    egui::ScrollArea::vertical().stick_to_bottom(true).show(ui, |ui| {
                        for (mine, m) in &ct.history {
                            let who = if *mine { RichText::new("Me").color(RED) } else { RichText::new(&ct.nick).color(Color32::BLUE) };
                            ui.label(who.strong());
                            ui.label(m);
                        }
                    });
                });
                if ctx.input(|i| i.viewport().close_requested()) { ct.open = false; }
            });
        }
    }
}

fn main() -> eframe::Result {
    eframe::run_native("ICQ",
        eframe::NativeOptions { viewport: ViewportBuilder::default().with_inner_size([220.0, 480.0]), ..Default::default() },
        Box::new(|_| Ok(Box::new(App::new()))))
}
