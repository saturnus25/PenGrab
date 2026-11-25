use crossterm::{
    cursor::{MoveTo, Show},
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    style::{Color, ResetColor, SetBackgroundColor},
    terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::io;

#[derive(Clone, Copy)]
enum Screen {
    Main,
    Alternate,
}

pub struct Session {
    mouse: bool,
    screen: Option<Screen>,
    #[cfg(windows)]
    original: windows_state::State,
}

impl Session {
    pub fn enter(mouse: bool, dark: bool) -> io::Result<Self> {
        let mut session = Self {
            mouse: false,
            screen: None,
            #[cfg(windows)]
            original: windows_state::State::capture()?,
        };
        terminal::enable_raw_mode()?;

        // The classic console and WinPTY can report stale dimensions in the alternate buffer.
        let alternate = std::env::var_os("WT_SESSION").is_some_and(|value| !value.is_empty());
        if alternate && execute!(io::stdout(), EnterAlternateScreen).is_ok() {
            session.screen = Some(Screen::Alternate);
        } else {
            session.screen = Some(Screen::Main);
        }
        if dark {
            session.set_background(true)?;
        }
        if dark || matches!(session.screen, Some(Screen::Main)) {
            execute!(io::stdout(), Clear(ClearType::All), MoveTo(0, 0))?;
        }
        let _ = session.set_mouse(mouse);
        Ok(session)
    }

    pub fn set_background(&self, dark: bool) -> io::Result<()> {
        if dark {
            execute!(io::stdout(), SetBackgroundColor(Color::Black))
        } else {
            execute!(io::stdout(), ResetColor)?;
            #[cfg(windows)]
            self.original.restore_attributes()?;
            Ok(())
        }
    }

    pub fn set_mouse(&mut self, mouse: bool) -> io::Result<()> {
        match (self.mouse, mouse) {
            (false, true) => execute!(io::stdout(), EnableMouseCapture)?,
            (true, false) => execute!(io::stdout(), DisableMouseCapture)?,
            _ => {}
        }
        self.mouse = mouse;
        Ok(())
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.set_mouse(false);
        match self.screen {
            Some(Screen::Alternate) => {
                let _ = execute!(io::stdout(), ResetColor, LeaveAlternateScreen, Show);
            }
            Some(Screen::Main) => {
                let _ = self.set_background(false);
                let _ = execute!(io::stdout(), Clear(ClearType::All), MoveTo(0, 0), Show);
            }
            None => {}
        }
        #[cfg(windows)]
        self.original.restore();
        #[cfg(not(windows))]
        let _ = terminal::disable_raw_mode();
    }
}

pub fn clean_text(text: &str) -> String {
    text.chars()
        .map(|letter| match letter {
            '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' => ' ',
            letter if letter.is_control() => ' ',
            letter => letter,
        })
        .collect()
}

#[cfg(windows)]
mod windows_state {
    use crossterm_winapi::{Console, ConsoleMode, Handle, ScreenBuffer};
    use std::io;

    pub struct State {
        input: ConsoleMode,
        output: ConsoleMode,
        original_input: u32,
        original_output: u32,
        attributes: u16,
    }

    impl State {
        pub fn capture() -> io::Result<Self> {
            let input = ConsoleMode::from(Handle::current_in_handle()?);
            let output = ConsoleMode::from(Handle::current_out_handle()?);
            Ok(Self {
                original_input: input.mode()?,
                original_output: output.mode()?,
                attributes: ScreenBuffer::current()?.info()?.attributes(),
                input,
                output,
            })
        }

        pub fn restore_attributes(&self) -> io::Result<()> {
            Console::output()?.set_text_attribute(self.attributes)
        }

        pub fn restore(&self) {
            let _ = self.restore_attributes();
            let _ = self.output.set_mode(self.original_output);
            let _ = self.input.set_mode(self.original_input);
        }
    }
}
