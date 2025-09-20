use iced::{
    mouse, subscription,
    widget::{button, column, container, image, text},
    window::{self, Position},
    Application, Command, Element, Event, Point, Settings, Subscription, Theme,
};
use snip_core::capture_fullscreen;

pub fn run() -> iced::Result {
    SnipApp::run(Settings {
        window: window::Settings {
            size: (800, 600),
            position: Position::Centered,
            decorations: false,
            level: window::Level::AlwaysOnTop,
            ..Default::default()
        },
        ..Default::default()
    })
}

#[derive(Default)]
struct SnipApp {
    screenshot: Option<image::Handle>,
    dragging: bool,
}

#[derive(Debug, Clone)]
enum Message {
    TakeScreenshot,
    ScreenshotOk(Vec<u8>),
    ScreenshotErr,
    MouseDown,
    MouseUp,
    MouseMoved(Point),
    Noop,
}

impl Application for SnipApp {
    type Message = Message;
    type Theme = Theme;
    type Executor = iced::executor::Default;
    type Flags = ();

    fn new(_flags: ()) -> (Self, Command<Message>) {
        (
            Self {
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
            Message::MouseDown => {
                self.dragging = true;
            }
            Message::MouseUp => {
                self.dragging = false;
            }
            Message::MouseMoved(Point) => {}
            Message::Noop => {}
        }
        Command::none()
    }

    fn view(&self) -> Element<'_, Message> {
        if let Some(img) = &self.screenshot {
            container(column![image(img.clone()),].spacing(10).padding(10)).into()
        } else {
            column![
                text("Snip Rust Screenshot Tool").size(32),
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

    fn subscription(&self) -> Subscription<Message> {
        subscription::events().map(|event| match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => Message::MouseDown,
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => Message::MouseUp,
            Event::Mouse(mouse::Event::CursorMoved { position }) => Message::MouseMoved(position),
            _ => Message::Noop,
        })
    }
}
