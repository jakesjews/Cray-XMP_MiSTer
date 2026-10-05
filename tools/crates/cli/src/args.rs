//! A small command line parser shared by the subcommands.

/// Errors that mean the command line itself is wrong start with this, so
/// `main` can add the usage line and pick the usage exit status.
pub const USAGE_PREFIX: &str = "bad arguments: ";

/// Parsed arguments: positional ones in order, and options with their values.
pub struct Args {
    pub positional: Vec<String>,
    options: Vec<(String, String)>,
}

impl Args {
    /// Parse `argv`.  Every name in `with_value` is an option that takes a
    /// value, given as `-o FILE`, `--start 16` or `--start=16`.  Anything
    /// else that starts with `-` is an error.
    pub fn parse(argv: &[String], with_value: &[&str]) -> Result<Args, String> {
        let mut args = Args {
            positional: Vec::new(),
            options: Vec::new(),
        };
        let mut it = argv.iter();
        while let Some(a) = it.next() {
            if !a.starts_with('-') || a == "-" {
                args.positional.push(a.clone());
                continue;
            }
            let (name, inline) = match a.split_once('=') {
                Some((n, v)) if n.starts_with("--") => (n, Some(v.to_string())),
                _ => (a.as_str(), None),
            };
            if !with_value.contains(&name) {
                return Err(format!("{}unknown option {}", USAGE_PREFIX, name));
            }
            let value = match inline {
                Some(v) => v,
                None => it
                    .next()
                    .cloned()
                    .ok_or_else(|| format!("{}{} needs a value", USAGE_PREFIX, name))?,
            };
            args.options.push((name.to_string(), value));
        }
        Ok(args)
    }

    /// The last value given for an option.
    pub fn value(&self, name: &str) -> Option<&str> {
        self.options
            .iter()
            .rev()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }

    /// Every value given for an option, in order.
    pub fn values(&self, name: &str) -> Vec<&str> {
        self.options
            .iter()
            .filter(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
            .collect()
    }

    /// The single positional argument, described as `what` in errors.
    pub fn one_positional(&self, what: &str) -> Result<&str, String> {
        match self.positional.as_slice() {
            [one] => Ok(one),
            [] => Err(format!("{}{} is missing", USAGE_PREFIX, what)),
            _ => Err(format!("{}more than one {} given", USAGE_PREFIX, what)),
        }
    }

    /// A number: decimal, or octal with a `0o` prefix, or hex with `0x`.
    pub fn number(&self, name: &str) -> Result<Option<u64>, String> {
        let Some(text) = self.value(name) else {
            return Ok(None);
        };
        let parsed = if let Some(o) = text.strip_prefix("0o") {
            u64::from_str_radix(o, 8)
        } else if let Some(x) = text.strip_prefix("0x") {
            u64::from_str_radix(x, 16)
        } else {
            text.parse()
        };
        parsed.map(Some).map_err(|_| {
            format!(
                "{}{} wants a number (decimal, 0o octal or 0x hex), not `{}`",
                USAGE_PREFIX, name, text
            )
        })
    }
}
