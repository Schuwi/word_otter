use std::ops::AddAssign;

use rand::{
    distributions::uniform::{SampleUniform, UniformSampler},
    Rng,
};
use rug::ops::CompleteRound;

pub const BIGINT_LIB: &str = "rug";

pub type BigInteger = rug::Integer;

#[derive(Debug, Default, Clone, Copy)]
pub struct RichEntropy {
    /// The number of bits of entropy in the number of variations
    ///
    /// Don't quote me on the soundness of this calculation
    pub entropy_bits: f32,
    /// The exponent of the log10 of the number of variations
    ///
    /// This is useful for displaying the number of variations in scientific notation
    /// Example: 1234500 = 1.2345e6 => 6
    pub variations_exponent: u32,
    /// The mantissa of the log10 of the number of variations
    ///
    /// This is useful for displaying the number of variations in scientific notation
    /// Example: 1234500 = 1.2345e6 => 1.2345
    pub variations_mantissa: f32,
}

// Source: https://gitlab.com/tspiteri/rug/-/blob/cf96b2c811ccff258ec1483400c0fc8ceff973a6/src/integer/traits.rs#L335-344
fn float_from_int(i: &BigInteger) -> rug::Float {
    let abs = i.as_abs();
    let mut prec = abs.significant_bits();
    // avoid copying trailing zeros
    if let Some(zeros) = abs.find_one(0) {
        prec -= zeros;
    }
    prec = prec.max(rug::float::prec_min());
    rug::Float::with_val(prec, i)
}

impl RichEntropy {
    pub fn calculate(variations: BigInteger) -> Self {
        let variations = float_from_int(&variations);
        let precision = variations.prec();

        let log2 = variations.clone().log2().to_f32();
        let variations_exponent = variations
            .clone()
            .log10()
            .floor()
            .to_u32_saturating()
            .unwrap();
        let variations_mantissa = (variations
            / rug::Float::u_pow_u(10, variations_exponent).complete(precision))
        .to_f32();

        RichEntropy {
            entropy_bits: log2,
            variations_exponent,
            variations_mantissa,
        }
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Default, Clone)]
pub struct IntegerWrapper(pub BigInteger);

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

pub struct RugUniformSampler {
    low: BigInteger,
    range: BigInteger,
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
