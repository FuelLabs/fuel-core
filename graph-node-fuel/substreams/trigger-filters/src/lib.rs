use anyhow::anyhow;
use std::collections::HashSet;

#[derive(Debug, Default, PartialEq)]
pub struct NearFilter<'a> {
    pub accounts: HashSet<&'a str>,
    pub partial_accounts: HashSet<(Option<&'a str>, Option<&'a str>)>,
}

impl<'a> NearFilter<'a> {
    pub fn matches(&self, account: &str) -> bool {
        let partial_match = self.partial_accounts.iter().any(|partial| match partial {
            (Some(prefix), Some(suffix)) => {
                account.starts_with(prefix) && account.ends_with(suffix)
            }
            (Some(prefix), None) => account.starts_with(prefix),
            (None, Some(suffix)) => account.ends_with(suffix),
            (None, None) => unreachable!(),
        });

        if !self.accounts.contains(&account) && !partial_match {
            return false;
        }

        true
    }
}

impl<'a> TryFrom<&'a str> for NearFilter<'a> {
    type Error = anyhow::Error;

    fn try_from(params: &'a str) -> Result<Self, Self::Error> {
        let mut accounts: HashSet<&str> = HashSet::default();
        let mut partial_accounts: HashSet<(Option<&str>, Option<&str>)> = HashSet::default();
        let mut lines = params.lines();
        let mut header = lines
            .next()
            .ok_or(anyhow!("header line not present"))?
            .split(",");
        let accs_len: usize = header
            .next()
            .ok_or(anyhow!("header didn't have the expected format"))?
            .parse()
            .map_err(|_| anyhow!("accounts len is supposed to be a usize"))?;
        let partials_len: usize = header
            .next()
            .ok_or(anyhow!("header didn't contain patials len"))?
            .parse()
            .map_err(|_| anyhow!("partials len is supposed to be a usize"))?;

        let accs_line = lines.next();
        if accs_len != 0 {
            accounts.extend(
                accs_line
                    .ok_or(anyhow!("full matches line not found"))?
                    .split(","),
            );
        }

        if partials_len != 0 {
            partial_accounts.extend(lines.take(partials_len).map(|line| {
                let mut parts = line.split(",");
                let start = match parts.next() {
                    Some(x) if x.is_empty() => None,
                    x => x,
                };
                let end = match parts.next() {
                    Some(x) if x.is_empty() => None,
                    x => x,
                };
                (start, end)
            }));
        }

        Ok(NearFilter {
            accounts,
            partial_accounts,
        })
    }
}
