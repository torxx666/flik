use clap::Parser;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use flik::app::App;
use flik::format::{LogFormat, ViewMode, parse_field_selection_query};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use std::io;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "flik")]
#[command(author = "Flik Team")]
#[command(version = "0.1.0")]
#[command(about = "Ultra-fast log analyzer (Apache, K8s, JSON) inspired by Norton Commander", long_about = None)]
struct Cli {
    /// Log file to analyze
    #[arg(value_name = "FILE")]
    file: PathBuf,

    /// Optional second log file, opened in an independent split pane
    #[arg(value_name = "SECOND_FILE")]
    second_file: Option<PathBuf>,

    /// Start directly in follow mode (tail -F)
    #[arg(short = 'f', long = "follow")]
    follow: bool,

    /// Start directly in table / columns view
    #[arg(short = 't', long = "table")]
    table: bool,

    /// Initial fields / column range to display (e.g. "5-7", "3,6", "timestamp,level")
    #[arg(short = 'c', long = "fields")]
    fields: Option<String>,

    /// Force log format (json, apache, apache_error, k8s, syslog, raw)
    #[arg(long = "format")]
    format: Option<String>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let is_stdin = cli.file.to_str() == Some("-") || cli.file.to_str() == Some("/dev/stdin");

    if is_stdin && std::io::IsTerminal::is_terminal(&io::stdin()) {
        eprintln!("Error: No data piped to stdin.");
        eprintln!("Usage: <command> | flik -");
        eprintln!("Example: docker logs -f my_container | flik -");
        eprintln!("Or provide a file directly: flik /path/to/log.log");
        std::process::exit(1);
    }

    // Exclusive creation with private permissions; retain ownership for cleanup on errors.
    let temp_stdin_file = if is_stdin {
        Some(
            tempfile::Builder::new()
                .prefix("flik_stdin_")
                .suffix(".log")
                .tempfile()?,
        )
    } else {
        None
    };
    let temp_stdin_path = temp_stdin_file
        .as_ref()
        .map(|file| file.path().to_path_buf());

    let target_file = if let Some(ref p) = temp_stdin_path {
        p.clone()
    } else {
        if !cli.file.exists() {
            eprintln!("Error: File '{}' does not exist.", cli.file.display());
            std::process::exit(1);
        }
        cli.file.clone()
    };

    #[cfg(unix)]
    let stop_pipe_flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    #[cfg(unix)]
    let _pipe_thread = if is_stdin {
        use std::os::unix::io::AsRawFd;
        let pipe_fd = unsafe { libc::dup(0) };
        if pipe_fd < 0 {
            eprintln!("Error: Failed to duplicate stdin file descriptor.");
            std::process::exit(1);
        }

        let tty = match std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/tty")
        {
            Ok(f) => f,
            Err(e) => {
                eprintln!(
                    "Error: Cannot open /dev/tty for interactive terminal: {}",
                    e
                );
                std::process::exit(1);
            }
        };

        unsafe {
            libc::dup2(tty.as_raw_fd(), 0);
        }

        let thread_stop = stop_pipe_flag.clone();
        let thread_path = target_file.clone();
        Some(std::thread::spawn(move || {
            use std::io::{BufRead, BufReader, Write};
            use std::os::unix::io::FromRawFd;
            let file = unsafe { std::fs::File::from_raw_fd(pipe_fd) };
            let mut reader = BufReader::new(file);
            let mut out = match std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&thread_path)
            {
                Ok(f) => f,
                Err(_) => return,
            };
            let mut line = String::new();
            while !thread_stop.load(std::sync::atomic::Ordering::Relaxed) {
                line.clear();
                match reader.read_line(&mut line) {
                    Ok(0) => {
                        // Source temporarily quiet or reached EOF
                        std::thread::sleep(std::time::Duration::from_millis(50));
                        if thread_stop.load(std::sync::atomic::Ordering::Relaxed) {
                            break;
                        }
                    }
                    Ok(_) => {
                        let _ = out.write_all(line.as_bytes());
                        let _ = out.flush();
                    }
                    Err(_) => break,
                }
            }
        }))
    } else {
        None
    };

    if is_stdin {
        // Allow initial lines to stream in before UI launches
        std::thread::sleep(std::time::Duration::from_millis(60));
    }

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Set panic hook to restore terminal in case of unexpected panic
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture);
        original_hook(panic_info);
    }));

    // Initialize application
    let mut app = match App::new(target_file) {
        Ok(app) => app,
        Err(e) => {
            disable_raw_mode()?;
            execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture)?;
            eprintln!("Error opening file: {}", e);
            std::process::exit(1);
        }
    };

    configure_app(&mut app, &cli, is_stdin);

    // Run event loop
    let run_res = (|| -> io::Result<()> {
        let mut workspace = flik::workspace::Workspace::new(app);
        if let Some(path) = &cli.second_file {
            workspace.open_other(path)?;
            configure_app(&mut workspace.panes[1], &cli, false);
            workspace.active = 0;
        }
        workspace.run(&mut terminal)
    })();

    // Clean up stdin streaming
    if is_stdin {
        #[cfg(unix)]
        stop_pipe_flag.store(true, std::sync::atomic::Ordering::Relaxed);
        if let Some(ref p) = temp_stdin_path {
            let _ = std::fs::remove_file(p);
        }
    }

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = run_res {
        return Err(err.into());
    }

    Ok(())
}

fn configure_app(app: &mut App, cli: &Cli, is_stdin: bool) {
    if cli.follow || is_stdin {
        app.reader.set_tail_enabled(true);
        app.ui_state.tail_following = true;
    }
    if cli.table {
        app.format.view_mode = ViewMode::ColumnsTable;
    }
    if let Some(ref fmt) = cli.format {
        match fmt.to_lowercase().as_str() {
            "json" => app.format.selected_format = LogFormat::Json,
            "nginx" | "nginx_access" | "apache" | "apache_combined" => app.format.selected_format = LogFormat::ApacheCombined,
            "apache_common" => app.format.selected_format = LogFormat::ApacheCommon,
            "apache_error" => app.format.selected_format = LogFormat::ApacheError,
            "nginx_error" => app.format.selected_format = LogFormat::NginxError,
            "k8s" | "kubernetes" => app.format.selected_format = LogFormat::Kubernetes,
            "syslog" => app.format.selected_format = LogFormat::Syslog,
            "raw" => app.format.selected_format = LogFormat::Raw,
            _ => {}
        }
        app.filter.sync_field_filter(
            &app.format.visible_fields,
            &app.format.available_fields,
            app.format.is_json(),
            &app.reader,
        );
    }
    if let Some(ref f) = cli.fields {
        let selected = parse_field_selection_query(f, &app.format.available_fields);
        if !selected.is_empty() {
            app.format.visible_fields = selected;
            app.format.view_mode = ViewMode::ColumnsTable;
            app.filter.sync_field_filter(
                &app.format.visible_fields,
                &app.format.available_fields,
                app.format.is_json(),
                &app.reader,
            );
        }
    }
}
