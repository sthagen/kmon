mod app;
mod event;
mod kernel;
mod util;
use app::{App, Blocks};
use enum_unitary::{Bounded, EnumUnitary};
use event::{Event, Events};
use kernel::log::KernelLogs;
use kernel::lkm::{KernelModules, ScrollDirection};
use std::io::stdout;
use termion::event::Key;
use termion::input::MouseTerminal;
use termion::raw::IntoRawMode;
use termion::screen::AlternateScreen;
use tui::backend::TermionBackend;
use tui::layout::{Constraint, Direction, Layout};
use tui::Terminal;
use unicode_width::UnicodeWidthStr;

const VERSION: &'static str = "0.1.0"; /* Version */
const REFRESH_RATE: &'static str = "250"; /* Default refresh rate of the terminal */

/**
 * Create a terminal instance with using termion as backend.
 *
 * @param  ArgMatches
 * @return Result
 */
fn create_term(args: &clap::ArgMatches) -> Result<(), failure::Error> {
    /* Configure the terminal. */
    let stdout = stdout().into_raw_mode()?;
    let stdout = MouseTerminal::from(stdout);
    let stdout = AlternateScreen::from(stdout);
    let backend = TermionBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let events = Events::new(
        args.value_of("rate")
            .unwrap_or(REFRESH_RATE)
            .parse::<u64>()
            .unwrap(),
    );
    terminal.hide_cursor()?;
    /* Set required items for the terminal widgets. */
    let mut app = App::new(Blocks::ModuleTable);
    let mut kernel_logs = KernelLogs::new();
    let mut kernel_modules = KernelModules::new(args);
    kernel_modules.scroll_list(ScrollDirection::Top);
    /* Draw terminal and render the widgets. */
    loop {
        terminal.draw(|mut f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(75), Constraint::Percentage(25)].as_ref())
                .split(f.size());
            {
                let chunks = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Percentage(60), Constraint::Percentage(40)].as_ref())
                    .split(chunks[0]);
                {
                    let chunks = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints([Constraint::Length(3), Constraint::Percentage(100)].as_ref())
                        .split(chunks[0]);
                    {
                        let chunks = Layout::default()
                            .direction(Direction::Horizontal)
                            .constraints(
                                [Constraint::Percentage(60), Constraint::Percentage(40)].as_ref(),
                            )
                            .split(chunks[0]);
                        app.draw_search_input(&mut f, chunks[0], &events.tx);
                        app.draw_kernel_version(&mut f, chunks[1], &kernel_logs.version)
                    }
                    app.draw_kernel_modules(&mut f, chunks[1], &mut kernel_modules);
                }
                app.draw_module_info(&mut f, chunks[1], &mut kernel_modules);
            }
            app.draw_kernel_activities(&mut f, chunks[1], &mut kernel_logs);
        })?;
        /* Set cursor position if the search mode flag is set. */
        if app.search_mode {
            util::set_cursor_pos(
                terminal.backend_mut(),
                2 + app.search_query.width() as u16,
                2,
            )?;
        }
        /* Handle terminal events. */
        match events.rx.recv()? {
            /* Key input events. */
            Event::Input(input) => {
                if !app.search_mode {
                    /* Default input mode. */
                    match input {
                        /* Quit. */
                        Key::Char('q') | Key::Char('Q') | Key::Ctrl('c') | Key::Ctrl('d') => {
                            break;
                        }
                        /* Refresh. */
                        Key::Char('r') | Key::Char('R') => {
                            app = App::new(Blocks::ModuleTable);
                            kernel_logs.scroll_offset = 0;
                            kernel_modules = KernelModules::new(args);
                            kernel_modules.scroll_list(ScrollDirection::Top);
                        }
                        /* Scroll the selected block up. */
                        Key::Up | Key::Char('k') | Key::Char('K') => match app.selected_block {
                            Blocks::ModuleTable => kernel_modules.scroll_list(ScrollDirection::Up),
                            Blocks::ModuleInfo => {
                                kernel_modules.scroll_mod_info(ScrollDirection::Up)
                            }
                            Blocks::Activities => {
                                events.tx.send(Event::Input(Key::PageUp)).unwrap();
                            }
                            _ => {}
                        },
                        /* Scroll the selected block down. */
                        Key::Down | Key::Char('j') | Key::Char('J') => match app.selected_block {
                            Blocks::ModuleTable => {
                                kernel_modules.scroll_list(ScrollDirection::Down)
                            }
                            Blocks::ModuleInfo => {
                                kernel_modules.scroll_mod_info(ScrollDirection::Down)
                            }
                            Blocks::Activities => {
                                events.tx.send(Event::Input(Key::PageDown)).unwrap();
                            }
                            _ => {}
                        },
                        /* Select the next terminal block. */
                        Key::Left | Key::Char('h') | Key::Char('H') | Key::Ctrl('h') => {
                            app.selected_block = match app.selected_block.prev_variant() {
                                Some(v) => v,
                                None => Blocks::max_value(),
                            }
                        }
                        /* Select the previous terminal block. */
                        Key::Right | Key::Char('l') | Key::Char('L') | Key::Ctrl('l') => {
                            app.selected_block = match app.selected_block.next_variant() {
                                Some(v) => v,
                                None => Blocks::min_value(),
                            }
                        }
                        /* Scroll to the top of the module list. */
                        Key::Char('t') | Key::Char('T') => {
                            app.selected_block = Blocks::ModuleTable;
                            kernel_modules.scroll_list(ScrollDirection::Top)
                        }
                        /* Scroll to the bottom of the module list. */
                        Key::Char('b') | Key::Char('B') => {
                            app.selected_block = Blocks::ModuleTable;
                            kernel_modules.scroll_list(ScrollDirection::Bottom)
                        }
                        /* Scroll kernel activities up. */
                        Key::PageUp => {
                            app.selected_block = Blocks::Activities;
                            if kernel_logs.scroll_offset > 2 {
                                kernel_logs.scroll_offset -= 3;
                            }
                        }
                        /* Scroll kernel activities down. */
                        Key::PageDown => {
                            app.selected_block = Blocks::Activities;
                            if kernel_logs.output.len() > 0 {
                                kernel_logs.scroll_offset += 3;
                                kernel_logs.scroll_offset %=
                                    (kernel_logs.output.lines().count() as u16) * 2;
                            }
                        }
                        /* Scroll module information up. */
                        Key::Backspace => kernel_modules.scroll_mod_info(ScrollDirection::Up),
                        /* Scroll module information down. */
                        Key::Char(' ') => kernel_modules.scroll_mod_info(ScrollDirection::Down),
                        /* Search in modules. */
                        Key::Char('\n') | Key::Char('s') | Key::Char('/') | Key::Home => {
                            app.selected_block = Blocks::SearchInput;
                            if input != Key::Char('\n') {
                                app.search_query = String::new();
                            }
                            util::set_cursor_pos(
                                terminal.backend_mut(),
                                2 + app.search_query.width() as u16,
                                2,
                            )?;
                            terminal.show_cursor()?;
                            app.search_mode = true;
                        }
                        _ => {}
                    }
                } else {
                    /* Search mode. */
                    match input {
                        /* Quit with ctrl+key combinations. */
                        Key::Ctrl('c') | Key::Ctrl('d') => {
                            break;
                        }
                        /* Exit search mode. */
                        Key::Char('\n')
                        | Key::Right
                        | Key::Ctrl('l')
                        | Key::Left
                        | Key::Ctrl('h') => {
                            /* Select the next or previous block. */
                            app.selected_block = match input {
                                Key::Left | Key::Ctrl('h') => {
                                    match app.selected_block.prev_variant() {
                                        Some(v) => v,
                                        None => Blocks::max_value(),
                                    }
                                }
                                _ => Blocks::ModuleTable,
                            };
                            /* Show the first modules information. */
                            if kernel_modules.index == 0 {
                                kernel_modules.scroll_list(ScrollDirection::Top);
                            }
                            /* Hide terminal cursor and set search mode flag. */
                            terminal.hide_cursor()?;
                            app.search_mode = false;
                        }
                        /* Append character to search query. */
                        Key::Char(c) => {
                            app.search_query.push(c);
                            kernel_modules.index = 0;
                        }
                        /* Delete last character from search query. */
                        Key::Backspace => {
                            app.search_query.pop();
                            kernel_modules.index = 0;
                        }
                        _ => {}
                    }
                }
            }
            /* Kernel events. */
            Event::Kernel(logs) => {
                kernel_logs.output = logs;
            }
            _ => {}
        }
    }
    Ok(())
}

/**
 * Entry point.
 */
fn main() {
    let matches = util::parse_args(VERSION);
    create_term(&matches).expect("failed to create terminal");
}
