use std::{collections::HashMap, num::NonZeroUsize, str::FromStr};

use color_eyre::eyre::{bail, Result};
use itertools::Itertools as _;
use rand::{distributions::WeightedIndex, Rng, SeedableRng};
use unicode_normalization::UnicodeNormalization as _;
use wasm_bindgen::prelude::wasm_bindgen;

use crate::bigint::{BigInteger, IntegerWrapper};

#[wasm_bindgen]
pub struct RngWrapper(#[wasm_bindgen(skip)] pub rand::rngs::StdRng);

#[wasm_bindgen]
impl RngWrapper {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        RngWrapper(rand::rngs::StdRng::from_entropy())
    }
}

#[derive(Debug, Default, Clone, serde::Deserialize)]
#[wasm_bindgen(getter_with_clone)]
pub struct RichWord {
    pub word: String,
    #[serde(default)]
    pub meanings: Vec<String>,
}

#[wasm_bindgen]
pub struct PreprocessOptions {
    pub keep_case: bool,
    pub use_umlauts: bool,
    pub min_word_length: Option<usize>,
    #[wasm_bindgen(skip)]
    pub exclude_regexes: Vec<regex::Regex>,
}

#[wasm_bindgen]
impl PreprocessOptions {
    /// Creates a new [`PreprocessOptions`] object with the given options.
    ///
    /// # Arguments
    ///
    /// * `keep_case` - Controls whether words should be lower-cased.
    /// * `use_umlauts` - Controls whether words with umlauts are filtered out.
    /// * `min_word_length` - Controls whether words with insufficient length are removed.
    #[wasm_bindgen(constructor)]
    pub fn new(keep_case: bool, use_umlauts: bool, min_word_length: Option<usize>) -> Self {
        PreprocessOptions {
            keep_case,
            use_umlauts,
            min_word_length,
            exclude_regexes: Vec::new(),
        }
    }

    /// Adds a regex to the list of regexes that will be used to exclude words.
    ///
    /// Matching words will be removed in preprocessing.
    ///
    /// # Returns
    /// An error string describing what went wrong if the regex is invalid.
    #[wasm_bindgen]
    pub fn add_exclude_regex(&mut self, regex: &str) -> Result<(), String> {
        let mut builder = regex::RegexBuilder::new(regex);
        builder.case_insensitive(true);
        let regex = builder
            .build()
            .map_err(|err| format!("Invalid regex: {}", err))?;
        self.exclude_regexes.push(regex);
        Ok(())
    }
}

#[wasm_bindgen]
pub fn preprocess_word_list(words: Vec<RichWord>, options: &PreprocessOptions) -> Vec<RichWord> {
    words
        .into_iter()
        .filter(|word| {
            if let Some(min_word_length) = options.min_word_length {
                word.word.len() >= min_word_length
            } else {
                true
            }
        })
        .filter(|word| {
            if !options.use_umlauts {
                word.word.is_ascii()
            } else {
                true
            }
        })
        .filter(|word| {
            let mut keep = true;
            for reg in &options.exclude_regexes {
                if reg.is_match(&word.word) {
                    keep = false
                }
            }
            keep
        })
        .map(|mut word| {
            if !options.keep_case {
                word.word = word.word.to_lowercase();
            }
            word
        })
        .collect()
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
    pub fn build_database(mut words: Vec<RichWord>) -> Option<Self> {
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

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(getter_with_clone)]
pub struct GenerationResult {
    pub words: Vec<RichWord>,
    pub variations: js_sys::BigInt,
}

#[cfg(target_arch = "wasm32")]
impl GenerationResult {
    fn new(words: Vec<RichWord>, variations: BigInteger) -> Self {
        let variations = js_sys::BigInt::from_str(&variations.to_string()).unwrap();
        GenerationResult {
            words,
            variations,
        }
    }
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn generate_words(
    rng: &mut RngWrapper,
    input_words: Vec<RichWord>,
    words: usize,
    max_length: usize,
) -> Result<GenerationResult, String> {
    let (words, variations) =
        generate_words_impl(rng, input_words, words, max_length).map_err(|err| err.to_string())?;
    Ok(GenerationResult::new(words, variations))
}

#[cfg(not(target_arch = "wasm32"))]
pub fn generate_words(
    rng: &mut RngWrapper,
    input_words: Vec<RichWord>,
    words: usize,
    max_length: usize,
) -> Result<(Vec<RichWord>, BigInteger)> {
    generate_words_impl(rng, input_words, words, max_length)
}

#[allow(non_snake_case)]
fn generate_words_impl(
    rng: &mut RngWrapper,
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

        let group_len = 1 + rng.0.sample(&distribution);
        let group = algorithm
            .word_db
            .get_group(NonZeroUsize::new(group_len).unwrap());
        let index = rng.0.gen_range(0..group.len());
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

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn generate_words_naive(
    rng: &mut RngWrapper,
    input_words: Vec<RichWord>,
    words: usize,
    max_length: Option<usize>,
) -> Result<GenerationResult, String> {
    let (words, variations) = generate_words_naive_impl(rng, input_words, words, max_length)
        .map_err(|err| err.to_string())?;
    Ok(GenerationResult::new(words, variations))
}

#[cfg(not(target_arch = "wasm32"))]
pub fn generate_words_naive(
    rng: &mut RngWrapper,
    input_words: Vec<RichWord>,
    words: usize,
    max_length: Option<usize>,
) -> Result<(Vec<RichWord>, BigInteger)> {
    generate_words_naive_impl(rng, input_words, words, max_length)
}

fn generate_words_naive_impl(
    rng: &mut RngWrapper,
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
        let word_index = rng.0.gen_range(0..input_words.len());
        out_words.push(input_words[word_index].clone());
        variations *= input_words.len();
    }

    Ok((out_words, variations))
}
