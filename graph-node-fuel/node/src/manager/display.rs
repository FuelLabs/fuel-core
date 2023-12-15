pub struct List {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

impl List {
    pub fn new(headers: Vec<&str>) -> Self {
        let headers = headers.into_iter().map(|s| s.to_string()).collect();
        Self {
            headers,
            rows: Vec::new(),
        }
    }

    pub fn append(&mut self, row: Vec<String>) {
        if row.len() != self.headers.len() {
            panic!(
                "there are {} headers but the row has {} entries: {:?}",
                self.headers.len(),
                row.len(),
                row
            );
        }
        self.rows.push(row);
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    pub fn render(&self) {
        const LINE_WIDTH: usize = 78;

        let header_width = self.headers.iter().map(|h| h.len()).max().unwrap_or(0);
        let header_width = if header_width < 5 { 5 } else { header_width };
        let mut first = true;
        for row in &self.rows {
            if !first {
                println!(
                    "{:-<width$}-+-{:-<rest$}",
                    "",
                    "",
                    width = header_width,
                    rest = LINE_WIDTH - 3 - header_width
                );
            }
            first = false;

            for (header, value) in self.headers.iter().zip(row) {
                println!("{:width$} | {}", header, value, width = header_width);
            }
        }
    }
}
