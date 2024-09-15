use std::{
    collections::HashMap,
    fs::File,
    io::{BufRead, BufReader},
    num::NonZeroUsize,
    path::PathBuf,
};

use bigint::{BigInteger, IntegerWrapper};
use clap::Parser;
use color_eyre::{
    eyre::{bail, eyre, Context},
    Result,
};
use itertools::Itertools as _;
use rand::{distributions::WeightedIndex, Rng, SeedableRng};
use regex::RegexBuilder;
use unicode_normalization::UnicodeNormalization;

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

    let words = read_wordlist(&args.word_list)?;

    // short-circuit if they want an empty password
    if words_count == 0 || max_len_no_seps == Some(0) {
        println!("");
        return Ok(());
    }

    let mut exclude_regexes = Vec::with_capacity(args.exclude.len());

    for regex_string in args.exclude {
        let mut builder = RegexBuilder::new(&regex_string);
        builder.case_insensitive(true);
        let regex = builder.build()?;
        exclude_regexes.push(regex);
    }

    let words: Vec<RichWord> = words
        .into_iter()
        .filter(|word| {
            if let Some(min_word_length) = args.min_word_length {
                word.word.len() >= min_word_length
            } else {
                true
            }
        })
        .filter(|word| {
            if !args.use_umlauts {
                word.word.is_ascii()
            } else {
                true
            }
        })
        .filter(|word| {
            let mut keep = true;
            for reg in &exclude_regexes {
                if reg.is_match(&word.word) {
                    keep = false
                }
            }
            keep
        })
        .map(|mut word| {
            if !args.keep_case {
                word.word = word.word.to_lowercase();
            }
            word
        })
        .collect();

    // generate words for passphrase
    let mut password = String::new();
    let mut rng = rand::rngs::StdRng::from_entropy();

    let (words, mut variations) = if args.naive || max_len_no_seps.is_none() {
        generate_words_naive(&mut rng, words, words_count, max_len_no_seps)?
    } else {
        generate_words(&mut rng, words, words_count, max_len_no_seps.unwrap())?
    };

    // assemble password
    for i in 0..words_count {
        password.push_str(words[i].word.as_str());

        if i != words_count - 1 {
            if let Some(sep_char) = args.sep_char {
                password.push(sep_char);
            } else {
                let digit = rng.gen_range(0..=9);
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

        // print entropy
        let entropy = bigint::RichEntropy::calculate(variations);
        eprintln!(
            "Entropy: {:.1} bits ({:.3}e{} possible variations)",
            entropy.entropy_bits, entropy.variations_mantissa, entropy.variations_exponent
        );

        // print meanings
        if !args.no_meanings {
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

#[derive(Debug, Default, Clone, serde::Deserialize)]
struct RichWord {
    word: String,
    #[serde(default)]
    meanings: Vec<String>,
}

fn parse_json_wordlist(reader: impl BufRead) -> Result<Vec<RichWord>> {
    let words: Vec<RichWord> = serde_json::from_reader(reader)?;
    Ok(words)
}

#[derive(Debug)]
struct WordDb {
    word_groups: HashMap<NonZeroUsize, Vec<String>>,
    min_length: NonZeroUsize,
    meanings: HashMap<String, Vec<String>>,
}

impl WordDb {
    ///
    /// Returns None if words is empty or only contains empty strings.
    ///
    fn build_database(mut words: Vec<RichWord>) -> Option<Self> {
        // run unicode normalization on all words
        words = words
            .into_iter()
            .map(|RichWord { word, meanings }| RichWord {
                word: word.nfc().collect(),
                meanings,
            })
            .collect();
        // sort words alphabetically
        words.sort_unstable_by(|a, b| a.word.cmp(&b.word));
        // merge duplicates
        words = words
            .into_iter()
            .coalesce(|mut a, b| {
                if a.word == b.word {
                    a.meanings.extend(b.meanings);
                    Ok(a)
                } else {
                    Err((a, b))
                }
            })
            .collect();
        // remove 0-length strings
        if words
            .first()
            .map(|word| word.word.is_empty())
            .unwrap_or(false)
        {
            words.remove(0);
        }

        if words.is_empty() {
            return None;
        }

        let mut map = HashMap::new();
        let mut meanings = HashMap::new();
        let mut min_length: NonZeroUsize = words[0].word.len().try_into().expect("no empty words");
        let mut max_length: NonZeroUsize = min_length;

        for RichWord {
            word,
            meanings: word_meanings,
        } in words
        {
            let length = word.len().try_into().expect("no empty words");

            let group_vec = map.entry(length).or_insert(Vec::new());
            group_vec.push(word.clone());

            meanings
                .entry(word)
                .and_modify(|vec: &mut Vec<String>| vec.extend_from_slice(&word_meanings))
                .or_insert(word_meanings);

            if length > max_length {
                max_length = length;
            }
            if length < min_length {
                min_length = length;
            }
        }

        for group_len in 1..max_length.get() {
            let group_len = NonZeroUsize::new(group_len).unwrap();

            let _ignored = map.entry(group_len).or_insert(Vec::new());
        }

        Some(WordDb {
            word_groups: map,
            min_length,
            meanings,
        })
    }

    fn get_group(&self, len: NonZeroUsize) -> &Vec<String> {
        self.word_groups.get(&len).unwrap()
    }

    ///
    /// n_len: Returns the number of words with the given length.
    ///
    fn group_size(&self, len: NonZeroUsize) -> usize {
        if let Some(group_vec) = self.word_groups.get(&len) {
            group_vec.len()
        } else {
            0
        }
    }

    fn shortest_group_len(&self) -> NonZeroUsize {
        self.min_length
    }

    fn attach_meanings(&self, words: &[String]) -> Vec<RichWord> {
        words
            .iter()
            .map(|word| RichWord {
                word: word.clone(),
                meanings: self.meanings.get(word).cloned().unwrap_or_default(),
            })
            .collect()
    }
}

struct Algorithm {
    word_db: WordDb,
    memoize_variations_for_length: HashMap<u32, BigInteger>,
    memoize_unreachable_variations_at_depth: HashMap<(u32, u32), BigInteger>,
}

#[allow(non_snake_case)]
impl Algorithm {
    fn new(word_db: WordDb) -> Self {
        Algorithm {
            word_db,
            memoize_variations_for_length: Default::default(),
            memoize_unreachable_variations_at_depth: Default::default(),
        }
    }

    fn variations_for_length(&mut self, max_length: u32) -> &BigInteger {
        fn variations_for_length_impl(
            word_db: &WordDb,
            memoization: &HashMap<u32, BigInteger>,
            max_length: u32,
        ) -> BigInteger {
            if max_length <= 0 {
                BigInteger::from(1)
            } else {
                let mut sum = BigInteger::ZERO;

                for group_len in 1..=max_length {
                    let n_k = word_db.group_size(
                        NonZeroUsize::new(group_len.try_into().expect("iterator over range 1.."))
                            .expect("iterator over range 1.."),
                    );

                    let f_x_minus_k = memoization
                        .get(&(max_length - group_len))
                        .expect("must have been calculated before");

                    sum += n_k * f_x_minus_k;
                }

                sum
            }
        }

        let memoization = &mut self.memoize_variations_for_length;

        if !memoization.contains_key(&max_length) {
            // begin calculating values from the bottom up
            for max_length_ in 0..=max_length {
                if !memoization.contains_key(&(max_length_)) {
                    let value =
                        variations_for_length_impl(&self.word_db, &memoization, max_length_);
                    memoization.insert(max_length_, value);
                }
            }
        }

        memoization
            .get(&max_length)
            .expect("has just been calculated if it didn't exist")
    }

    fn unreachable_variations_at_depth(&mut self, max_length: u32, depth: u32) -> &BigInteger {
        fn unreachable_variations_at_depth_impl(
            word_db: &WordDb,
            memoization: &HashMap<(u32, u32), BigInteger>,
            memoization_variations: &HashMap<u32, BigInteger>,
            max_length: u32,
            depth: u32,
        ) -> BigInteger {
            if depth == 0 {
                let f_x = memoization_variations
                    .get(&(max_length))
                    .expect("must have been calculated before");

                f_x - BigInteger::from(1)
            } else {
                let mut sum = BigInteger::ZERO;

                for group_len in 1..=max_length {
                    let n_k = word_db.group_size(
                        NonZeroUsize::new(group_len.try_into().expect("iterator over range 1.."))
                            .expect("iterator over range 1.."),
                    );

                    let g_x_minus_k_minus_one_D_minus_one = memoization
                        .get(&(max_length - (group_len - 1), depth - 1))
                        .expect("must have been calculated before");

                    sum += n_k * g_x_minus_k_minus_one_D_minus_one;
                }

                sum
            }
        }

        // prime required values
        self.variations_for_length(max_length);

        let memoization = &mut self.memoize_unreachable_variations_at_depth;

        if !memoization.contains_key(&(max_length, depth)) {
            // begin calculating values from the bottom up
            for depth_ in 0..=depth {
                for max_length_ in 0..=max_length {
                    if !memoization.contains_key(&(max_length_, depth_)) {
                        let value = unreachable_variations_at_depth_impl(
                            &self.word_db,
                            &memoization,
                            &self.memoize_variations_for_length,
                            max_length_,
                            depth_,
                        );
                        memoization.insert((max_length_, depth_), value);
                    }
                }
            }
        }

        memoization
            .get(&(max_length, depth))
            .expect("has just been calculated if it didn't exist")
    }

    ///
    /// Returns the number of possible variations chaining this number of `words` up to a `max_length`.
    ///
    fn variations_for_length_and_depth(&mut self, max_length: u32, depth: u32) -> BigInteger {
        let f_x = self.variations_for_length(max_length).clone();
        let g_x_minus_D_D =
            self.unreachable_variations_at_depth(max_length.saturating_sub(depth), depth);

        f_x - g_x_minus_D_D
    }
}

#[allow(non_snake_case)]
fn generate_words(
    rng: &mut impl Rng,
    input_words: Vec<RichWord>,
    words: usize,
    max_length: usize,
) -> Result<(Vec<RichWord>, BigInteger)> {
    let word_db = match WordDb::build_database(input_words) {
        None => bail!("Input file contained no valid words"),
        Some(word_db) => word_db,
    };

    if words * word_db.shortest_group_len().get() > max_length {
        bail!("Length constraints cannot be fulfilled");
    }

    let mut generated_words: Vec<String> = Vec::with_capacity(words);
    let mut algorithm = Algorithm::new(word_db);

    // TODO unwrap
    let mut max_length = u32::try_from(max_length).unwrap();
    let mut words = u32::try_from(words).unwrap();

    // already calculates and memoizes all values used in the following loop
    let variations = algorithm.variations_for_length_and_depth(max_length, words);

    while words > 0 {
        let step_max_len: u32 = max_length - (words - 1);

        let distr_iter = (1..=step_max_len).map(|group_len| {
            let n_k = algorithm.word_db.group_size(
                NonZeroUsize::new(group_len.try_into().unwrap()).expect("iterator over range 1.."),
            );
            let f_dash_x_minus_k_D_minus_one =
                algorithm.variations_for_length_and_depth(step_max_len - group_len, words - 1);

            IntegerWrapper(n_k * f_dash_x_minus_k_D_minus_one)
        });
        let distribution = WeightedIndex::new(distr_iter).unwrap();

        let group_len = 1 + rng.sample(&distribution);
        let group = algorithm
            .word_db
            .get_group(NonZeroUsize::new(group_len).unwrap());
        let index = rng.gen_range(0..group.len());
        let word = group[index].clone();

        max_length -= u32::try_from(word.len()).unwrap();
        words -= 1;
        generated_words.push(word);
    }

    Ok((
        algorithm.word_db.attach_meanings(&generated_words),
        variations,
    ))
}

fn generate_words_naive(
    rng: &mut impl Rng,
    mut input_words: Vec<RichWord>,
    words: usize,
    max_length: Option<usize>,
) -> Result<(Vec<RichWord>, BigInteger)> {
    let max_word_length = max_length.map(|len| len / words);

    // run unicode normalization on all words and filter max length
    input_words = input_words
        .into_iter()
        .filter(|word| {
            if let Some(max_len) = max_word_length {
                word.word.len() <= max_len
            } else {
                true
            }
        })
        .map(|RichWord { word, meanings }| RichWord {
            word: word.nfc().collect(),
            meanings,
        })
        .collect();
    // sort words alphabetically
    input_words.sort_unstable_by(|a, b| a.word.cmp(&b.word));
    // merge duplicates
    input_words = input_words
        .into_iter()
        .coalesce(|mut a, b| {
            if a.word == b.word {
                a.meanings.extend(b.meanings);
                Ok(a)
            } else {
                Err((a, b))
            }
        })
        .collect();
    // remove 0-length strings
    if input_words
        .first()
        .map(|word| word.word.is_empty())
        .unwrap_or(false)
    {
        input_words.remove(0);
    }

    if input_words.is_empty() {
        bail!("Input file contained no valid words");
    }

    let mut out_words = Vec::with_capacity(words);
    let mut variations = BigInteger::from(1);

    for _ in 0..words {
        let word_index = rng.gen_range(0..input_words.len());
        out_words.push(input_words[word_index].clone());
        variations *= input_words.len();
    }

    Ok((out_words, variations))
}
