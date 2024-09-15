use std::{
    fs::File,
    io::{BufRead, BufReader},
    path::PathBuf,
};

use clap::Parser;
use color_eyre::{
    eyre::{eyre, Context},
    Result,
};
use implementation::RichWord;
use rand::Rng;
use regex::RegexBuilder;

#[cfg(all(
    any(target_arch = "x86", target_arch = "x86_64", target_arch = "aarch64"),
    not(feature = "dashu")
))]
#[path = "bigint_rug.rs"]
mod bigint;
#[cfg(not(all(
    any(target_arch = "x86", target_arch = "x86_64", target_arch = "aarch64"),
    not(feature = "dashu")
)))]
#[path = "bigint_dashu.rs"]
mod bigint;

mod implementation;

#[derive(Debug, clap::Parser)]
struct Args {
    /// Limit the resulting password to this length in bytes
    #[arg(long, short = 'L')]
    max_length: Option<usize>,
    /// Use a naive algorithm to limit password length
    ///
    /// Only affects results on passwords with limited max length.
    ///
    /// Instead of considering all words that together reach the max length this simple algorithm
    /// only considers words with a length <= max length / words, to make sure the generated password
    /// does not get longer than max length.
    #[arg(long)]
    naive: bool,
    /// Don't lowercase the words in the resulting password
    #[arg(long, short = 'c')]
    keep_case: bool,
    /// Include words with umlauts
    #[arg(long, short = 'u')]
    use_umlauts: bool,
    /// Only include words with a minimum length of this many bytes
    #[arg(long)]
    min_word_length: Option<usize>,
    /// Path to a list of words to assemble the password from
    ///
    /// Words must be separated by line breaks.
    #[arg(long, default_value = "./wortliste.txt")]
    word_list: PathBuf,
    /// Separate words by a fixed character instead of a random digit
    #[arg(long, short = 's')]
    sep_char: Option<char>,
    /// Words matching these regex pattern(s) are excluded from the word list
    ///
    /// For syntax see Rust's [regex](https://docs.rs/regex/latest/regex/) crate.
    ///
    /// The regex is applied before words are lowercased (see `keep_case`). The regex is thus
    /// compiled in case-insensitive mode but this can be overriden inside the pattern using the
    /// `(?-i)` syntax.
    #[arg(long)]
    exclude: Vec<String>,
    /// Don't print meanings of words
    ///
    /// By default the meanings of the words are printed to stderr if a .json word list is used.
    /// This flag disables this behavior.
    #[arg(long)]
    no_meanings: bool,
    /// Suppress all output except the password
    #[arg(long, short = 'q')]
    quiet: bool,
    /// How many words to use
    words: usize,
}

fn main() -> Result<()> {
    color_eyre::install()?;

    let args = Args::parse();

    // convert args
    let words_count = args.words;
    let separators_count = words_count.saturating_sub(1);
    let separator_length = args.sep_char.map(|c| c.len_utf8()).unwrap_or(1);
    let max_len_no_seps = args
        .max_length
        .map(|max_length| max_length.saturating_sub(separators_count * separator_length));

    let mut exclude_regexes = Vec::with_capacity(args.exclude.len());

    for regex_string in args.exclude {
        let mut builder = RegexBuilder::new(&regex_string);
        builder.case_insensitive(true);
        let regex = builder.build()?;
        exclude_regexes.push(regex);
    }

    // read word list
    let words = read_wordlist(&args.word_list)?;

    // short-circuit if they want an empty password
    if words_count == 0 || max_len_no_seps == Some(0) {
        println!();
        return Ok(());
    }

    // preprocess words
    let options = implementation::PreprocessOptions {
        keep_case: args.keep_case,
        use_umlauts: args.use_umlauts,
        min_word_length: args.min_word_length,
        exclude_regexes,
    };
    let words = implementation::preprocess_word_list(words, &options);

    // generate words for passphrase
    let mut password = String::new();
    let mut rng = implementation::RngWrapper::new();

    let (words, mut variations) = if args.naive || max_len_no_seps.is_none() {
        implementation::generate_words_naive(&mut rng, words, words_count, max_len_no_seps)?
    } else {
        implementation::generate_words(&mut rng, words, words_count, max_len_no_seps.unwrap())?
    };

    // assemble password
    for (i, word) in words.iter().enumerate() {
        password.push_str(word.word.as_str());

        if i != words_count - 1 {
            if let Some(sep_char) = args.sep_char {
                password.push(sep_char);
            } else {
                let digit = rng.0.gen_range(0..=9);
                password.push(char::from_digit(digit, 10).expect("digit is 0..=9"));
                variations *= 10;
            }
        }
    }

    println!("{}", password);

    // print additional information
    if !args.quiet {
        if cfg!(debug_assertions) {
            eprintln!("[Debug] Length: {}", password.len());
            eprintln!("[Debug] Using {} for calculations", bigint::BIGINT_LIB);
        }

        eprintln!();

        // print entropy
        let entropy = bigint::RichEntropy::calculate(variations);
        eprintln!(
            "Entropy: {:.1} bits ({:.3}e{} possible variations)",
            entropy.entropy_bits, entropy.variations_mantissa, entropy.variations_exponent
        );

        // print meanings
        if !args.no_meanings {
            eprintln!();

            for word in words {
                if !word.meanings.is_empty() {
                    eprintln!("Meanings for \"{}\":", word.word);
                    for meaning in word.meanings {
                        eprintln!("  - {}", meaning);
                    }
                }
            }
        }
    }

    Ok(())
}

fn read_wordlist(file_name: &PathBuf) -> Result<Vec<RichWord>> {
    enum FileType {
        Txt,
        Json,
        #[cfg(feature = "gzip")]
        JsonGz,
    }

    let file_type = file_name
        .extension()
        .and_then(|ext| ext.to_str())
        .and_then(|ext| match ext {
            "txt" => Some(FileType::Txt),
            "json" => Some(FileType::Json),
            #[cfg(feature = "gzip")]
            "gz" => file_name
                .file_stem()
                .and_then(|stem| std::path::Path::new(stem).extension())
                .and_then(|ext| ext.to_str())
                .filter(|ext| *ext == "json")
                .map(|_| FileType::JsonGz),
            _ => None,
        })
        .ok_or_else(|| {
            #[cfg(not(feature = "gzip"))]
            {
                eyre!("Unsupported word list file format. Must be .txt or .json")
            }
            #[cfg(feature = "gzip")]
            {
                eyre!("Unsupported word list file format. Must be .txt, .json or .json.gz")
            }
        })?;

    let file = File::open(file_name)
        .with_context(|| format!("Could not open word list file at '{}'", file_name.display()))?;

    match file_type {
        FileType::Json => {
            let reader = BufReader::new(file);
            parse_json_wordlist(reader)
        }
        FileType::Txt => {
            let reader = BufReader::new(file);
            parse_txt_wordlist(reader)
        }
        #[cfg(feature = "gzip")]
        FileType::JsonGz => {
            let buf_reader = BufReader::new(file);
            let gzip_reader = flate2::bufread::MultiGzDecoder::new(buf_reader);
            let reader = BufReader::new(gzip_reader);
            parse_json_wordlist(reader)
        }
    }
}

fn parse_txt_wordlist(reader: impl BufRead) -> Result<Vec<RichWord>> {
    let mut words: Vec<RichWord> = Vec::new();

    for word in reader.lines() {
        words.push(RichWord {
            word: word?,
            meanings: Vec::new(),
        });
    }

    Ok(words)
}

fn parse_json_wordlist(reader: impl BufRead) -> Result<Vec<RichWord>> {
    let words: Vec<RichWord> = serde_json::from_reader(reader)?;
    Ok(words)
}
