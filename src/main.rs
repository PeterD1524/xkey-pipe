use anyhow::{bail, Context, Result};
use clap::Parser;
use x11rb::{
    connection::Connection,
    protocol::{
        xproto::{self, ConnectionExt, CreateWindowAux, EventMask, KeyPressEvent, WindowClass},
        xtest::ConnectionExt as _,
        Event,
    },
};

#[derive(Debug)]
struct KeyEvent {
    r#type: u8,
    detail: u8,
}

fn xdotool<S, I>(cmd: S, args: I) -> Result<std::process::Output>
where
    S: AsRef<std::ffi::OsStr> + std::fmt::Display,
    I: IntoIterator<Item = S>,
{
    let output = std::process::Command::new("xdotool")
        .arg(&cmd)
        .args(args)
        .output()?;
    if !output.status.success() {
        bail!("{} failed: {:?}", cmd, output);
    }
    Ok(output)
}

fn select_window() -> Result<u32> {
    let cmd = "selectwindow";
    let output = xdotool(cmd, [])?;
    (|| -> Result<u32> {
        let s = String::from_utf8(output.stdout.clone())?;
        let s = match s.strip_suffix("\n") {
            Some(s) => s,
            None => &s,
        };
        let i = s.parse::<u32>()?;
        Ok(i)
    })()
    .with_context(|| format!("{} bad stdout: {:?}", cmd, output))
}

fn activate(window: u32) -> Result<()> {
    xdotool("windowactivate", ["--sync", &window.to_string()])?;
    Ok(())
}

#[derive(Parser)]
struct Cli {
    #[arg(long, default_value_t = 2)]
    delay: u32,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let destination = select_window()?;

    let (conn, screen_num) = x11rb::connect(None)?;
    let screen = &conn.setup().roots[screen_num];
    let win_id = conn.generate_id()?;
    for cookie in [
        conn.create_window(
            x11rb::COPY_DEPTH_FROM_PARENT,
            win_id,
            screen.root,
            0,
            0,
            256,
            256,
            0,
            WindowClass::INPUT_OUTPUT,
            x11rb::COPY_FROM_PARENT,
            &CreateWindowAux::new()
                .event_mask(EventMask::KEY_PRESS | EventMask::KEY_RELEASE)
                .background_pixel(screen.white_pixel),
        )?,
        conn.map_window(win_id)?,
    ] {
        cookie.check()?;
    }

    let mut events = vec![];
    loop {
        match conn.wait_for_event()? {
            Event::KeyPress(KeyPressEvent { detail: 36, .. }) => {
                break;
            }
            Event::KeyPress(event) | Event::KeyRelease(event) => {
                events.push(KeyEvent {
                    r#type: event.response_type,
                    detail: event.detail,
                });
            }
            Event::MappingNotify(event) => {
                dbg!(event);
            }
            other => {
                bail!("unexpected event: {:?}", other);
            }
        }
    }

    activate(destination)?;

    let mut cookies = vec![];
    for event in &events {
        let time = if event.r#type == xproto::KEY_RELEASE_EVENT {
            cli.delay
        } else {
            x11rb::CURRENT_TIME
        };
        cookies.push(conn.xtest_fake_input(event.r#type, event.detail, time, 0, 0, 0, 0)?);
    }
    for cookie in cookies {
        cookie.check()?;
    }

    Ok(())
}
