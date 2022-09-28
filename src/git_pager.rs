const DEFAULT_ENV: &[(&str, &str)] = &[("LESS", "FRX"), ("LV", "-c"), ("LESSCHARSET", "UTF-8")];

pub struct Pager {
    cmd: Option<std::process::Command>,
}

impl Pager {
    pub fn stdout(args: &str) -> Self {
        let cmd = atty::is(atty::Stream::Stdout)
            .then(|| parse(args))
            .flatten();
        Self { cmd }
    }

    pub fn start(&mut self) -> ActivePager {
        let stdout = std::io::stdout().lock();
        if let Some(cmd) = &mut self.cmd {
            // should use pager instead of stderr
            if let Ok(p) = cmd.spawn() {
                let stderr = atty::is(atty::Stream::Stderr).then(|| std::io::stderr().lock());
                ActivePager {
                    primary: stdout,
                    _secondary: stderr,
                    pager: Some(p),
                }
            } else {
                ActivePager {
                    primary: stdout,
                    _secondary: None,
                    pager: None,
                }
            }
        } else {
            ActivePager {
                primary: stdout,
                _secondary: None,
                pager: None,
            }
        }
    }
}

pub struct ActivePager {
    primary: std::io::StdoutLock<'static>,
    _secondary: Option<std::io::StderrLock<'static>>,
    pager: Option<std::process::Child>,
}

impl ActivePager {
    pub fn as_writer(&mut self) -> std::io::Result<&mut dyn std::io::Write> {
        if let Some(pager) = &mut self.pager {
            pager
                .stdin
                .as_mut()
                .map(|s| {
                    let s: &mut dyn std::io::Write = s;
                    s
                })
                .ok_or_else(|| {
                    std::io::Error::new(
                        std::io::ErrorKind::BrokenPipe,
                        "could not access pager stdin",
                    )
                })
        } else {
            Ok(&mut self.primary)
        }
    }
}

impl Drop for ActivePager {
    fn drop(&mut self) {
        if let Some(pager) = &mut self.pager {
            let _ = pager.wait();
        }
    }
}

fn parse(args: &str) -> Option<std::process::Command> {
    let mut args = shlex::Shlex::new(args);
    let cmd = args.next()?;
    if cmd == "cat" {
        return None;
    }
    let mut cmd = std::process::Command::new(cmd);
    cmd.stdin(std::process::Stdio::piped());
    cmd.args(args);
    cmd.envs(DEFAULT_ENV.iter().copied());
    Some(cmd)
}