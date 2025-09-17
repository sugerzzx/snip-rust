use iced::widget::{button, column, text};
use iced::{window, Application, Command, Element, Settings, Theme};

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
}

#[derive(Debug, Clone)]
enum Message {
    IncrementPressed,
    DecrementPressed,
}

impl Application for SnipApp {
    type Message = Message;
    type Theme = Theme;
    type Executor = iced::executor::Default;
    type Flags = ();

    fn new(_flags: ()) -> (Self, Command<Message>) {
        (Self::default(), Command::none())
    }

    fn title(&self) -> String {
        "Snip Rust - 截图工具".to_string()
    }

    fn update(&mut self, message: Message) -> Command<Message> {
        match message {
            Message::IncrementPressed => {
                self.counter += 1;
            }
            Message::DecrementPressed => {
                self.counter -= 1;
            }
        }
        Command::none()
    }

    fn view(&self) -> Element<Message> {
        column![
            text("Snip Rust Screenshot Tool").size(32),
            text(format!("Counter: {}", self.counter)).size(20),
            button("Increment").on_press(Message::IncrementPressed),
            button("Decrement").on_press(Message::DecrementPressed),
        ]
        .padding(20)
        .spacing(20)
        .into()
    }

    fn theme(&self) -> Theme {
        Theme::Light
    }
}
