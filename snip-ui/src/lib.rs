use iced::widget::{button, column, container, image, text};
use iced::{window, Application, Command, Element, Settings, Theme};
use snip_core::capture_fullscreen;
// use anyhow::Result; // 当前未使用

pub fn run() -> iced::Result {
    SnipApp::run(Settings {
        window: window::Settings {
            size: (800, 600),
            position: window::Position::Centered,
            ..Default::default()
        },
        ..Default::default()
    })
}

#[derive(Default)]
struct SnipApp {
    counter: i32,
    screenshot: Option<image::Handle>,
    scale: f32,
}

#[derive(Debug, Clone)]
enum Message {
    IncrementPressed,
    DecrementPressed,
    TakeScreenshot,
    ScreenshotOk(Vec<u8>),
    ScreenshotErr,
}

impl Application for SnipApp {
    type Message = Message;
    type Theme = Theme;
    type Executor = iced::executor::Default;
    type Flags = ();

    fn new(_flags: ()) -> (Self, Command<Message>) {
        (
            Self {
                scale: 1.0,
                ..Default::default()
            },
            Command::none(),
        )
    }

    fn title(&self) -> String {
        "Snip Rust - Screenshot Tool".to_string()
    }

    fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::IncrementPressed => {
                self.counter += 1;
            }
            Message::DecrementPressed => {
                self.counter -= 1;
            }
            Message::TakeScreenshot => {
                return Command::perform(async move { capture_fullscreen() }, |res| match res {
                    Ok(p) => Message::ScreenshotOk(p),
                    Err(_) => Message::ScreenshotErr,
                });
            }
            Message::ScreenshotOk(png) => {
                self.screenshot = Some(image::Handle::from_memory(png));
            }
            Message::ScreenshotErr => { /* TODO: 错误提示 */ }
        }
        Command::none()
    }

    fn view(&self) -> Element<Message> {
        if let Some(img) = &self.screenshot {
            container(
                column![
                    text("Screenshot Result:"),
                    image(img.clone()),
                    button("Retake Screenshot").on_press(Message::TakeScreenshot),
                ]
                .spacing(10)
                .padding(10),
            )
            .into()
        } else {
            column![
                text("Snip Rust Screenshot Tool").size(32),
                text(format!("Counter: {}", self.counter)).size(20),
                button("Increment").on_press(Message::IncrementPressed),
                button("Decrement").on_press(Message::DecrementPressed),
                button("Take Screenshot and Show").on_press(Message::TakeScreenshot),
            ]
            .padding(20)
            .spacing(20)
            .into()
        }
    }

    fn theme(&self) -> Theme {
        Theme::Light
    }
}
