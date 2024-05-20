use std::{
    collections::HashMap,
    fs::File,
    io::{BufRead, BufReader},
    num::NonZeroUsize,
    ops::AddAssign,
    path::PathBuf,
};

use clap::Parser;
use color_eyre::{eyre::bail, Result};
use rand::{
    distributions::{
        uniform::{SampleUniform, UniformSampler},
        WeightedIndex,
    },
    Rng, SeedableRng,
};
use regex::RegexBuilder;
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
    #[arg(long, default_value = "./wortliste.txt")]
    word_list: PathBuf,
    /// Separate words by a fixed character instead of a random digit
    #[arg(long)]
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

    let mut exclude_regexes = Vec::with_capacity(args.exclude.len());

    for regex_string in args.exclude {
        let mut builder = RegexBuilder::new(&regex_string);
        builder.case_insensitive(true);
        let regex = builder.build()?;
        exclude_regexes.push(regex);
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
        .filter(|word| {
            let mut keep = true;
            for reg in &exclude_regexes {
                if reg.is_match(word) {
                    keep = false
                }
            }
            keep
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

    let (words, mut variations) = if args.naive || max_len_no_seps.is_none() {
        generate_words_naive(&mut rng, words, words_count, max_len_no_seps)?
    } else {
        generate_words(&mut rng, words, words_count, max_len_no_seps.unwrap())?
    };

    for i in 0..words_count {
        password.push_str(words[i].as_str());

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

    eprintln!("[Debug] Length: {}", password.len());
    let log2 = {
        // not so sure about the mathematical soundness of this calculation
        let (partial, exp) = variations.to_f32_exp();
        let exp = exp as f32 - 1.;
        let partial = (partial * 2.).log2();

        exp + partial
    };
    eprintln!(
        "Entropy: {:.1} bits ({:.3e} possible variations)",
        log2, variations
    );

    Ok(())
}

#[derive(Debug)]
struct WordDb {
    word_groups: HashMap<NonZeroUsize, Vec<String>>,
    min_length: NonZeroUsize,
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

        for group_len in 1..max_length.get() {
            let group_len = NonZeroUsize::new(group_len).unwrap();

            let _ignored = map.entry(group_len).or_insert(Vec::new());
        }

        Some(WordDb {
            word_groups: map,
            min_length,
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
}

struct Algorithm {
    word_db: WordDb,
    memoize_variations_for_length: HashMap<u32, rug::Integer>,
    memoize_unreachable_variations_at_depth: HashMap<(u32, u32), rug::Integer>,
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

    fn variations_for_length(&mut self, max_length: u32) -> &rug::Integer {
        fn variations_for_length_impl(
            word_db: &WordDb,
            memoization: &HashMap<u32, rug::Integer>,
            max_length: u32,
        ) -> rug::Integer {
            if max_length <= 0 {
                rug::Integer::from(1)
            } else {
                let mut sum = rug::Integer::ZERO;

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

    fn unreachable_variations_at_depth(&mut self, max_length: u32, depth: u32) -> &rug::Integer {
        fn unreachable_variations_at_depth_impl(
            word_db: &WordDb,
            memoization: &HashMap<(u32, u32), rug::Integer>,
            memoization_variations: &HashMap<u32, rug::Integer>,
            max_length: u32,
            depth: u32,
        ) -> rug::Integer {
            if depth == 0 {
                let f_x = memoization_variations
                    .get(&(max_length))
                    .expect("must have been calculated before");

                f_x - rug::Integer::from(1)
            } else {
                let mut sum = rug::Integer::ZERO;

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
    fn variations_for_length_and_depth(&mut self, max_length: u32, depth: u32) -> rug::Integer {
        let f_x = self.variations_for_length(max_length).clone();
        let g_x_minus_D_D =
            self.unreachable_variations_at_depth(max_length.saturating_sub(depth), depth);

        f_x - g_x_minus_D_D
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Default, Clone)]
struct IntegerWrapper(rug::Integer);

impl SampleUniform for IntegerWrapper {
    type Sampler = RugUniformSampler;
}

impl AddAssign<&'_ IntegerWrapper> for IntegerWrapper {
    fn add_assign(&mut self, rhs: &'_ IntegerWrapper) {
        self.0.add_assign(&rhs.0)
    }
}

struct RngWrapper<'a, T: Rng + ?Sized>(&'a mut T);

impl<'a, T: Rng + ?Sized> rug::rand::ThreadRandGen for RngWrapper<'a, T> {
    fn gen(&mut self) -> u32 {
        self.0.next_u32()
    }
}

struct RugUniformSampler {
    low: rug::Integer,
    range: rug::Integer,
}

impl UniformSampler for RugUniformSampler {
    type X = IntegerWrapper;

    fn new<B1, B2>(low: B1, high: B2) -> Self
    where
        B1: rand::distributions::uniform::SampleBorrow<Self::X> + Sized,
        B2: rand::distributions::uniform::SampleBorrow<Self::X> + Sized,
    {
        let low = low.borrow().0.clone();
        let range = high.borrow().0.clone() - &low;

        RugUniformSampler { low, range }
    }

    fn new_inclusive<B1, B2>(low: B1, high: B2) -> Self
    where
        B1: rand::distributions::uniform::SampleBorrow<Self::X> + Sized,
        B2: rand::distributions::uniform::SampleBorrow<Self::X> + Sized,
    {
        let low = low.borrow().0.clone();
        let range = high.borrow().0.clone() - &low + 1;

        RugUniformSampler { low, range }
    }

    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> Self::X {
        let mut rng = RngWrapper(rng);
        let mut rng = rug::rand::ThreadRandState::new_custom(&mut rng);

        IntegerWrapper(self.range.clone().random_below(&mut rng) + &self.low)
    }
}

#[allow(non_snake_case)]
fn generate_words(
    rng: &mut impl Rng,
    input_words: Vec<String>,
    words: usize,
    max_length: usize,
) -> Result<(Vec<String>, rug::Integer)> {
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
    let variations =
        algorithm.variations_for_length_and_depth(max_length, words);

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

    Ok((generated_words, variations))
}

fn generate_words_naive(
    rng: &mut impl Rng,
    mut input_words: Vec<String>,
    words: usize,
    max_length: Option<usize>,
) -> Result<(Vec<String>, rug::Integer)> {
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
    let mut variations = rug::Integer::from(1);

    for _ in 0..words {
        let word_index = rng.gen_range(0..input_words.len());
        out_words.push(input_words[word_index].clone());
        variations *= input_words.len();
    }

    Ok((out_words, variations))
}
