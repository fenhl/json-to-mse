use {
    std::{
        collections::BTreeSet,
        env,
        fs::File,
        io::{
            self,
            Cursor,
            stdin,
            stdout
        },
        path::PathBuf,
        str::FromStr
    },
    termion::is_tty,
    crate::{
        Error,
        mse::DataFile
    }
};

//TODO add remaining flags/options from readme
const FLAGS: [(&str, Option<char>, fn(&mut ArgsRegular) -> Result<(), Error>); 1] = [
    ("verbose", Some('v'), verbose)
];

const OPTIONS: [(&str, Option<char>, fn(&mut ArgsRegular, &str) -> Result<(), Error>); 1] = [
    ("output", Some('o'), output)
];

pub(crate) enum Output {
    File(PathBuf),
    Stdout
}

impl FromStr for Output {
    type Err = Error;

    fn from_str(s: &str) -> Result<Output, Error> {
        Ok(if s == "=" {
            Output::Stdout
        } else {
            Output::File(s.parse()?)
        })
    }
}

impl Output {
    pub(crate) fn write_set_file(self, set_file: DataFile) -> Result<(), Error> {
        match self {
            Output::File(path) => {
                set_file.write_to(File::create(path)?)?;
            }
            Output::Stdout => {
                let mut buf = Cursor::<Vec<_>>::default();
                set_file.write_to(&mut buf)?;
                io::copy(&mut buf, &mut stdout())?;
            }
        }
        Ok(())
    }
}

pub(crate) struct ArgsRegular {
    pub(crate) all_command: bool,
    pub(crate) auto_card_numbers: bool,
    pub(crate) cards: BTreeSet<String>,
    pub(crate) copyright: String,
    pub(crate) output: Output,
    pub(crate) planes_output: Option<Output>,
    pub(crate) schemes_output: Option<Output>,
    pub(crate) set_code: String,
    pub(crate) vanguards_output: Option<Output>,
    pub(crate) verbose: bool
}

impl Default for ArgsRegular {
    fn default() -> ArgsRegular {
        ArgsRegular {
            all_command: false,
            auto_card_numbers: false,
            cards: BTreeSet::default(),
            copyright: format!("NOT FOR SALE"),
            output: Output::Stdout,
            planes_output: None,
            schemes_output: None,
            set_code: format!("PROXY"),
            vanguards_output: None,
            verbose: false
        }
    }
}

impl ArgsRegular {
    fn handle_line(&mut self, line: String) -> Result<(), Error> {
        if line.starts_with('-') {
            // no stdin support since pos args aren't paths/files
            if line.starts_with("--") {
                for (long, _, handler) in &FLAGS {
                    if line == format!("--{}", long) {
                        handler(self)?;
                        return Ok(());
                    }
                }
                for (long, _, handler) in &OPTIONS {
                    if line.starts_with(&format!("--{} ", long)) || line.starts_with(&format!("--{}=", long)) {
                        handler(self, &line[format!("--{} ", long).len()..])?;
                        return Ok(());
                    }
                }
                Err(Error::Args(format!("unknown option in stdin: {}", line)))
            } else {
                'short_flags: for (i, short_flag) in line.chars().enumerate().skip(1) {
                    for &(_, short, handler) in &FLAGS {
                        if let Some(short) = short {
                            if short_flag == short {
                                handler(self)?;
                                continue 'short_flags;
                            }
                        }
                    }
                    for &(_, short, handler) in &OPTIONS {
                        if let Some(short) = short {
                            if short_flag == short {
                                handler(self, &line.chars().skip(i + 1).collect::<String>())?;
                                break 'short_flags;
                            }
                        }
                    }
                    return Err(Error::Args(format!("unknown option: -{}", short_flag)));
                }
                Ok(())
            }
        } else {
            //TODO commands, comments, queries
            self.cards.insert(line);
            Ok(())
        }
    }
}

pub(crate) enum Args {
    Regular(ArgsRegular),
    Help,
    Version
}

enum HandleShortArgResult {
    Continue,
    Break,
    NoMatch
}

impl Args {
    pub(crate) fn new() -> Result<Args, Error> {
        let mut raw_args = env::args().skip(1);
        let mut args = ArgsRegular::default();
        while let Some(arg) = raw_args.next() {
            if arg.starts_with('-') {
                // no stdin support since pos args aren't paths/files
                if arg.starts_with("--") {
                    if Args::handle_long_arg(&arg, &mut raw_args, &mut args)? {
                        // handled
                    } else if arg == "--help" {
                        return Ok(Args::Help);
                    } else if arg == "--version" {
                        return Ok(Args::Version);
                    } else {
                        return Err(Error::Args(format!("unknown option: {}", arg)));
                    }
                } else {
                    for (i, short_flag) in arg.chars().enumerate().skip(1) {
                        match Args::handle_short_arg(short_flag, &arg.chars().skip(i + 1).collect::<String>(), &mut raw_args, &mut args)? {
                            HandleShortArgResult::Continue => continue,
                            HandleShortArgResult::Break => break,
                            HandleShortArgResult::NoMatch => match short_flag {
                                'h' => { return Ok(Args::Help); }
                                c => { return Err(Error::Args(format!("unknown option: -{}", c))); }
                            }
                        }
                    }
                }
            } else {
                //TODO commands, comments, queries
                args.cards.insert(arg);
            }
        }
        let stdin = stdin();
        if !is_tty(&stdin) {
            // also read card names/commands from stdin
            loop {
                let mut buf = String::default();
                if stdin.read_line(&mut buf)? == 0 { break; }
                args.handle_line(buf)?;
            }
        }
        Ok(Args::Regular(args))
    }

    fn handle_long_arg(arg: &str, raw_args: &mut impl Iterator<Item = String>, args: &mut ArgsRegular) -> Result<bool, Error> {
        for (long, _, handler) in &FLAGS {
            if arg == format!("--{}", long) {
                handler(args)?;
                return Ok(true);
            }
        }
        for (long, _, handler) in &OPTIONS {
            if arg == format!("--{}", long) {
                let value = raw_args.next().ok_or(Error::Args(format!("missing value for option: --{}", long)))?;
                handler(args, &value)?;
                return Ok(true);
            }
            let prefix = format!("--{}=", long);
            if arg.starts_with(&prefix) {
                let value = &arg[prefix.len()..];
                handler(args, value)?;
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn handle_short_arg(short_flag: char, remaining_arg: &str, raw_args: &mut impl Iterator<Item = String>, args: &mut ArgsRegular) -> Result<HandleShortArgResult, Error> {
        for &(_, short, handler) in &FLAGS {
            if let Some(short) = short {
                if short_flag == short {
                    handler(args)?;
                    return Ok(HandleShortArgResult::Continue);
                }
            }
        }
        for &(_, short, handler) in &OPTIONS {
            if let Some(short) = short {
                if short_flag == short {
                    if remaining_arg.is_empty() {
                        handler(args, &raw_args.next().ok_or(Error::Args(format!("missing value for option: -{}", short_flag)))?)?;
                    } else {
                        handler(args, remaining_arg)?;
                    };
                    return Ok(HandleShortArgResult::Break);
                }
            }
        }
        Ok(HandleShortArgResult::NoMatch)
    }
}

fn output(args: &mut ArgsRegular, out_path: &str) -> Result<(), Error> {
    args.output = out_path.parse()?;
    Ok(())
}

fn verbose(args: &mut ArgsRegular) -> Result<(), Error> {
    args.verbose = true;
    Ok(())
}
