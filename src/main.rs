use std::{
    fs::File,
    io::{BufRead, BufReader},
    path::PathBuf,
};

use clap::Parser;
use color_eyre::Result;
use rand::{SeedableRng, Rng};

#[derive(Debug, clap::Parser)]
struct Args {
    #[arg(long, short = 'L')]
    /// Limit the resulting password to this length in bytes
    max_length: Option<usize>,
    #[arg(long, short = 'c')]
    /// Don't lowercase the words in the resulting password
    keep_case: bool,
    #[arg(long, short = 'u')]
    /// Include words with umlauts
    use_umlauts: bool,
    #[arg(long, default_value = "wortliste.txt")]
    /// Path to a list of words to assemble the password from
    ///
    /// Words must be separated by line breaks.
    word_list: PathBuf,
    /// How many words to use
    words: usize,
}

// TODO: customizable separator

fn main() -> Result<()> {
    color_eyre::install()?;

    let args = Args::parse();

    // convert args
    let words_count = args.words;
    let separators_count = words_count.saturating_sub(1);
    let max_word_length = args
        .max_length
        .map(|max_length| max_length.saturating_sub(separators_count) / words_count);

    let words_file = File::open(args.word_list)?;
    let file_reader = BufReader::new(words_file);

    let mut words: Vec<String> = Vec::new();

    for word in file_reader.lines() {
        words.push(word?);
    }

    // short-circuit if they want an empty password
    if words_count == 0 || max_word_length == Some(0) {
        println!("");
        return Ok(());
    }

    words.sort_unstable();

    let words: Vec<String> = words
        .into_iter()
        .filter(|word| {
            if let Some(max_word_length) = max_word_length {
                word.len() <= max_word_length
            } else {
                true
            }
        })
        .filter(|word| {
            if !args.use_umlauts {
                word.is_ascii()
            } else {
                true
            }
        })
        .map(|word| {
            if !args.keep_case {
                word.to_lowercase()
            } else {
                word
            }
        })
        .collect();

    let mut password = String::new();
    let mut rng = rand::rngs::StdRng::from_entropy();

    for i in 0..words_count {
        let word = &words[rng.gen_range(0..words.len())];
        password.push_str(word.as_str());

        if i != words_count - 1 {
            password.push('-');
        }
    }

    println!("{}", password);

    // TODO unwrap
    let combinations = (words.len() as f64).powi(words_count.try_into().unwrap()) * 10. * 10. * 10.;
    eprintln!(
        "Entropy: {:.1} bits ({} possible combinations)",
        (combinations).log2(),
        combinations
    );

    Ok(())
}