// ICQ-style Signal client. Talks to signal-cli's JSON-RPC mode over stdin/stdout.
// Setup once: `brew install signal-cli && signal-cli link -n ICQ` (scan QR in Signal app),
// then run with SIGNAL_ACCOUNT=+15551234567.
use eframe::egui::{self, Color32, RichText, ViewportBuilder, ViewportId};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::process::{ChildStdin, Command, Stdio};
use std::sync::mpsc::{channel, Receiver};

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

#[derive(Debug, PartialEq)]
enum Event {
    Contacts(Vec<(String, String)>),          // (id, name)
    Message { id: String, name: String, text: String, mine: bool },
    Error(String),
}

// Parse one line of signal-cli JSON-RPC output.
fn parse(line: &str) -> Option<Event> {
    let v: Value = serde_json::from_str(line).ok()?;
    let s = |v: &Value| v.as_str().filter(|s| !s.is_empty()).map(String::from);
    if let Some(e) = v.get("error") {
        return Some(Event::Error(e["message"].as_str().unwrap_or("error").into()));
    }
    if v["id"] == 1 {
        let list = v["result"].as_array()?.iter().filter_map(|c| {
            let id = s(&c["number"]).or_else(|| s(&c["uuid"]))?;
            let name = s(&c["name"]).or_else(|| s(&c["profile"]["givenName"])).unwrap_or_else(|| id.clone());
            Some((id, name))
        }).collect();
        return Some(Event::Contacts(list));
    }
    let env = &v["params"]["envelope"];
    if v["method"] != "receive" { return None; }
    if let Some(text) = s(&env["dataMessage"]["message"]) {
        let id = s(&env["sourceNumber"]).or_else(|| s(&env["sourceUuid"]))?;
        let name = s(&env["sourceName"]).unwrap_or_else(|| id.clone());
        return Some(Event::Message { id, name, text, mine: false });
    }
    // Sent from my phone: show it in the chat too.
    let sent = &env["syncMessage"]["sentMessage"];
    let text = s(&sent["message"])?;
    let id = s(&sent["destinationNumber"]).or_else(|| s(&sent["destinationUuid"]))?;
    Some(Event::Message { name: id.clone(), id, text, mine: true })
}

struct Contact { id: String, nick: String, history: Vec<(bool, String)>, draft: String, open: bool, unread: bool }

struct App { me: Status, contacts: Vec<Contact>, rx: Receiver<Event>, stdin: ChildStdin, next_id: u64, error: Option<String> }

fn uh_oh() {
    // ponytail: system sound via afplay; bundle the real "uh-oh" wav when you have one.
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("afplay").arg("/System/Library/Sounds/Pop.aiff").spawn();
}

impl App {
    fn new(ctx: egui::Context, account: &str) -> std::io::Result<Self> {
        let mut child = Command::new("signal-cli").args(["-a", account, "jsonRpc"])
            .stdin(Stdio::piped()).stdout(Stdio::piped()).spawn()?;
        let stdout = child.stdout.take().unwrap();
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if let Some(ev) = parse(&line) { let _ = tx.send(ev); ctx.request_repaint(); }
            }
            let _ = tx.send(Event::Error("signal-cli exited".into()));
            ctx.request_repaint();
        });
        let mut app = App { me: Status::Online, contacts: vec![], rx, stdin: child.stdin.take().unwrap(), next_id: 2, error: None };
        app.rpc(1, "listContacts", json!({}));
        Ok(app)
    }

    fn rpc(&mut self, id: u64, method: &str, params: Value) {
        let line = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        if writeln!(self.stdin, "{line}").is_err() { self.error = Some("signal-cli not running".into()); }
    }

    fn contact(&mut self, id: &str, name: &str) -> &mut Contact {
        let i = match self.contacts.iter().position(|c| c.id == id) {
            Some(i) => i,
            None => {
                self.contacts.push(Contact { id: id.into(), nick: name.into(), history: vec![], draft: String::new(), open: false, unread: false });
                self.contacts.len() - 1
            }
        };
        &mut self.contacts[i]
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        while let Ok(ev) = self.rx.try_recv() {
            match ev {
                Event::Contacts(list) => for (id, name) in list { self.contact(&id, &name).nick = name.clone(); },
                Event::Message { id, name, text, mine } => {
                    let c = self.contact(&id, &name);
                    c.history.push((mine, text));
                    if !mine { c.unread = !c.open; uh_oh(); }
                }
                Event::Error(e) => self.error = Some(e),
            }
        }

        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                flower(ui, self.me, 18.0);
                // ponytail: Signal has no presence; status is cosmetic.
                egui::ComboBox::from_id_salt("me").selected_text(self.me.label()).show_ui(ui, |ui| {
                    for s in Status::ALL { ui.selectable_value(&mut self.me, s, s.label()); }
                });
            });
        });
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.label(RichText::new("ICQ").strong().size(20.0).color(GREEN));
            if let Some(e) = &self.error { ui.colored_label(RED, e); }
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| {
                egui::CollapsingHeader::new(RichText::new("Contacts").strong()).default_open(true).show(ui, |ui| {
                    for ct in &mut self.contacts {
                        ui.horizontal(|ui| {
                            flower(ui, Status::Online, 14.0);
                            let text = if ct.unread { RichText::new(&ct.nick).strong().color(RED) } else { RichText::new(&ct.nick) };
                            if ui.add(egui::Label::new(text).sense(egui::Sense::click())).double_clicked() {
                                ct.open = true;
                                ct.unread = false;
                            }
                        });
                    }
                });
            });
        });

        let mut outgoing = vec![];
        for ct in self.contacts.iter_mut().filter(|c| c.open) {
            let title = format!("{} ({}) - Message Session", ct.nick, ct.id);
            ctx.show_viewport_immediate(ViewportId::from_hash_of(&ct.id),
                ViewportBuilder::default().with_title(title).with_inner_size([360.0, 320.0]), |ctx, _| {
                egui::TopBottomPanel::bottom("send").show(ctx, |ui| {
                    ui.add(egui::TextEdit::multiline(&mut ct.draft).desired_rows(3).desired_width(f32::INFINITY));
                    let enter = ui.input(|i| i.key_pressed(egui::Key::Enter) && i.modifiers.command);
                    if (ui.button("Send").clicked() || enter) && !ct.draft.trim().is_empty() {
                        let msg = std::mem::take(&mut ct.draft).trim().to_string();
                        ct.history.push((true, msg.clone()));
                        outgoing.push((ct.id.clone(), msg));
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
        for (to, msg) in outgoing {
            let id = self.next_id;
            self.next_id += 1;
            self.rpc(id, "send", json!({"recipient": [to], "message": msg}));
        }
    }
}

fn main() -> eframe::Result {
    let account = std::env::args().nth(1).or_else(|| std::env::var("SIGNAL_ACCOUNT").ok())
        .expect("usage: icq +15551234567 (or set SIGNAL_ACCOUNT); link first with `signal-cli link -n ICQ`");
    eframe::run_native("ICQ",
        eframe::NativeOptions { viewport: ViewportBuilder::default().with_inner_size([220.0, 480.0]), ..Default::default() },
        Box::new(move |cc| Ok(Box::new(App::new(cc.egui_ctx.clone(), &account).expect("failed to start signal-cli (brew install signal-cli)")))))
}

#[test]
fn parses_signal_cli() {
    let contacts = r#"{"jsonrpc":"2.0","id":1,"result":[{"number":"+1","name":"","profile":{"givenName":"Al"}},{"number":null,"uuid":"u2","name":"Bo"}]}"#;
    assert_eq!(parse(contacts), Some(Event::Contacts(vec![("+1".into(), "Al".into()), ("u2".into(), "Bo".into())])));
    let recv = r#"{"jsonrpc":"2.0","method":"receive","params":{"envelope":{"sourceNumber":"+1","sourceName":"Al","dataMessage":{"message":"hi"}}}}"#;
    assert_eq!(parse(recv), Some(Event::Message { id: "+1".into(), name: "Al".into(), text: "hi".into(), mine: false }));
    let sync = r#"{"jsonrpc":"2.0","method":"receive","params":{"envelope":{"syncMessage":{"sentMessage":{"destinationNumber":"+1","message":"yo"}}}}}"#;
    assert_eq!(parse(sync), Some(Event::Message { id: "+1".into(), name: "+1".into(), text: "yo".into(), mine: true }));
    assert_eq!(parse(r#"{"jsonrpc":"2.0","method":"receive","params":{"envelope":{"typingMessage":{}}}}"#), None);
    assert!(matches!(parse(r#"{"jsonrpc":"2.0","id":3,"error":{"message":"bad"}}"#), Some(Event::Error(_))));
}
