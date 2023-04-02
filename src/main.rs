use std::{
    cell::RefCell,
    collections::HashMap,
    fs::File,
    io::{BufRead, BufReader},
    num::NonZeroUsize,
    path::PathBuf,
};

use clap::Parser;
use color_eyre::{eyre::bail, Result};
use rand::{distributions::WeightedIndex, Rng, SeedableRng};
use unicode_normalization::UnicodeNormalization;

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
    ///
    /// Defaults to "./wortliste.txt".
    #[arg(long, default_value = "./wortliste.txt")]
    word_list: PathBuf,
    /// Separate words by a fixed character instead of a random digit
    #[arg(long)]
    sep_char: Option<char>,
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
    let separator_length = args.sep_char.map(|c| c.len_utf8()).unwrap_or(1);
    let max_len_no_seps = args
        .max_length
        .map(|max_length| max_length.saturating_sub(separators_count * separator_length));

    let words_file = File::open(args.word_list)?;
    let file_reader = BufReader::new(words_file);

    let mut words: Vec<String> = Vec::new();

    for word in file_reader.lines() {
        words.push(word?);
    }

    // short-circuit if they want an empty password
    if words_count == 0 || max_len_no_seps == Some(0) {
        println!("");
        return Ok(());
    }

    let words: Vec<String> = words
        .into_iter()
        .filter(|word| {
            if let Some(min_word_length) = args.min_word_length {
                word.len() >= min_word_length
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

    let (words, mut variations) = if args.naive {
        generate_words_naive(&mut rng, words, words_count, max_len_no_seps)?
    } else {
        generate_words(&mut rng, words, words_count, max_len_no_seps)?
    };

    for i in 0..words_count {
        password.push_str(words[i].as_str());

        if i != words_count - 1 {
            if let Some(sep_char) = args.sep_char {
                password.push(sep_char);
            } else {
                let digit = rng.gen_range(0..=9);
                password.push(char::from_digit(digit, 10).expect("digit is 0..=9"));
                variations *= 10.;
            }
        }
    }

    println!("{}", password);

    eprintln!("[Debug] Length: {}", password.len());
    eprintln!(
        "Entropy: {:.1} bits ({:.3e} possible variations)",
        variations.log2(),
        variations
    );

    Ok(())
}

#[derive(Debug)]
struct WordDb {
    word_groups: HashMap<NonZeroUsize, Vec<String>>,
    min_length: NonZeroUsize,
    max_length: NonZeroUsize,
    // memoize_count_length_le: RefCell<HashMap<NonZeroUsize, usize>>,
    memoize_count_variations: RefCell<HashMap<(usize, Option<usize>), f64>>,
}

impl WordDb {
    ///
    /// Returns None if words is empty or only contains empty strings.
    ///
    fn build_database(mut words: Vec<String>) -> Option<Self> {
        // run unicode normalization on all words
        words = words.into_iter().map(|word| word.nfc().collect()).collect();
        // sort words alphabetically
        words.sort_unstable();
        // remove duplicates
        words.dedup();
        // remove 0-length strings
        if matches!(words.first(), Some(word) if word.is_empty()) {
            words.remove(0);
        }

        if words.is_empty() {
            return None;
        }

        let mut map = HashMap::new();
        let mut min_length: NonZeroUsize = words[0].len().try_into().expect("no empty words");
        let mut max_length: NonZeroUsize = min_length;

        for word in words {
            let length = word.len().try_into().expect("no empty words");

            let group_vec = map.entry(length).or_insert(Vec::new());
            group_vec.push(word);

            if length > max_length {
                max_length = length;
            }
            if length < min_length {
                min_length = length;
            }
        }

        for group_len in min_length.get()..max_length.get() {
            let group_len = NonZeroUsize::new(group_len).unwrap();

            let _ignored = map.entry(group_len).or_insert(Vec::new());
        }

        Some(WordDb {
            word_groups: map,
            min_length,
            max_length,
            // memoize_count_length_le: RefCell::new(HashMap::new()),
            memoize_count_variations: RefCell::new(HashMap::new()),
        })
    }

    fn get_group(&self, len: NonZeroUsize) -> &Vec<String> {
        self.word_groups.get(&len).unwrap()
    }

    ///
    /// E_n: Returns the number of words with the given length.
    ///
    fn count_length_exact(&self, len: NonZeroUsize) -> usize {
        let group_vec = self.word_groups.get(&len).unwrap();

        group_vec.len()
    }

    ///
    /// N_n: Returns the number of words with less then or equal the given length.
    ///
    // fn count_length_le(&self, len: NonZeroUsize) -> usize {
    //     let mut memoization = self.memoize_count_length_le.borrow_mut();

    //     if !memoization.contains_key(&len) {
    //         let value = {
    //             let min_len = self.shortest_group_len();
    //             let max_len = std::cmp::min(len, self.longest_group_len());
    //             let mut count = 0;

    //             for group_len in min_len.get()..=max_len.get() {
    //                 let group_len = NonZeroUsize::new(group_len).unwrap();
    //                 let group_vec = self.word_groups.get(&group_len).unwrap();

    //                 count += group_vec.len();
    //             }

    //             count
    //         };
    //         memoization.insert(len, value);
    //     }

    //     *memoization.get(&len).unwrap()
    // }

    fn shortest_group_len(&self) -> NonZeroUsize {
        self.min_length
    }

    fn longest_group_len(&self) -> NonZeroUsize {
        self.max_length
    }

    ///
    /// Returns the number of possible variations chaining this number of `words` up to a `max_length`.
    ///
    fn count_variations(&self, words: usize, max_length: Option<usize>) -> f64 {
        if let Some(max_length) = max_length {
            debug_assert!(
                words * self.shortest_group_len().get() <= max_length,
                "{} * {} must be <= {}",
                words,
                self.shortest_group_len(),
                max_length
            );
        }

        let memoization = self.memoize_count_variations.borrow();
        if let Some(value) = memoization.get(&(words, max_length)) {
            *value
        } else {
            drop(memoization);

            let value = if words == 0 {
                1f64
            } else {
                let min_len = self.shortest_group_len().get();
                let step_max_len: usize = if let Some(max_length) = max_length {
                    max_length - (words - 1) * self.shortest_group_len().get()
                } else {
                    self.longest_group_len().get()
                };

                let mut variations = 0f64;
                for group_len in min_len..=step_max_len {
                    let group_len = NonZeroUsize::new(group_len).unwrap();
                    variations += self.count_length_exact(group_len) as f64
                        * self.count_variations(
                            words - 1,
                            max_length.map(|max_length| max_length - group_len.get()),
                        )
                }

                variations
            };

            self.memoize_count_variations
                .borrow_mut()
                .insert((words, max_length), value);

            value
        }
    }
}

fn generate_words(
    rng: &mut impl Rng,
    input_words: Vec<String>,
    words: usize,
    mut max_length: Option<usize>,
) -> Result<(Vec<String>, f64)> {
    let word_db = match WordDb::build_database(input_words) {
        None => bail!("Input file contained no valid words"),
        Some(word_db) => word_db,
    };

    if let Some(max_length) = max_length {
        if words * word_db.shortest_group_len().get() > max_length {
            bail!("Length constraints cannot be fulfilled");
        }
    }

    let mut generated_words: Vec<String> = Vec::with_capacity(words);
    let mut variations = None;

    for words in (1..=words).rev() {
        let step_max_len: usize = if let Some(max_length) = max_length {
            max_length - (words - 1) * word_db.shortest_group_len().get()
        } else {
            word_db.longest_group_len().get()
        };

        let distr_iter = (word_db.shortest_group_len().get()..=step_max_len).map(|group_len| {
            word_db.count_length_exact(group_len.try_into().unwrap()) as f64
                * word_db.count_variations(words - 1, max_length.map(|len| len - group_len))
        });
        let distribution = WeightedIndex::new(distr_iter.clone()).unwrap();

        let group_len = word_db.shortest_group_len().get() + rng.sample(&distribution);
        let group = word_db.get_group(NonZeroUsize::new(group_len).unwrap());
        let index = rng.gen_range(0..group.len());
        let word = group[index].clone();

        if variations == None {
            variations = Some(distr_iter.sum::<f64>());
        }
        max_length = max_length.map(|len| len - word.len());
        generated_words.push(word);
    }

    Ok((generated_words, variations.unwrap()))
}

fn generate_words_naive(
    rng: &mut impl Rng,
    mut input_words: Vec<String>,
    words: usize,
    max_length: Option<usize>,
) -> Result<(Vec<String>, f64)> {
    let max_word_length = max_length.map(|len| len / words);

    // run unicode normalization on all words and filter max length
    input_words = input_words
        .into_iter()
        .filter(|word| {
            if let Some(max_len) = max_word_length {
                word.len() <= max_len
            } else {
                true
            }
        })
        .map(|word| word.nfc().collect())
        .collect();
    // sort words alphabetically
    input_words.sort_unstable();
    // remove duplicates
    input_words.dedup();
    // remove 0-length strings
    if matches!(input_words.first(), Some(word) if word.is_empty()) {
        input_words.remove(0);
    }

    if input_words.is_empty() {
        bail!("Input file contained no valid words");
    }

    let mut out_words = Vec::with_capacity(words);
    let mut variations = 1f64;

    for _ in 0..words {
        let word_index = rng.gen_range(0..input_words.len());
        out_words.push(input_words[word_index].clone());
        variations *= input_words.len() as f64;
    }

    Ok((out_words, variations))
}
