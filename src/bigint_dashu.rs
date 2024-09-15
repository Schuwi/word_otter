use std::ops::AddAssign;

use dashu::integer::rand::UniformIBig;
use rand::{
    distributions::uniform::{SampleUniform, UniformSampler},
    Rng,
};

pub const BIGINT_LIB: &str = "dashu";

pub type BigInteger = dashu::Integer;

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

impl RichEntropy {
    pub fn calculate(variations: BigInteger) -> Self {
        // TODO: I don't quite trust the results of the log2 calculation
        // TODO: The calculations seem to get stuck for big inputs (e.g. 1000 words)

        let variations = dashu::Decimal::from(variations);
        let log2 = variations.ln() / dashu::Decimal::from(2).ln();

        let log10 = (variations.ln() / dashu::Decimal::from(10).ln())
            .floor()
            .to_int()
            .value();
        let mantissa = variations / dashu::Decimal::from(10).powi(log10.clone());

        RichEntropy {
            entropy_bits: log2.to_f32().value(),
            variations_exponent: log10.to_f32().value() as u32,
            variations_mantissa: mantissa.to_f32().value(),
        }
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Default, Clone)]
pub struct IntegerWrapper(pub BigInteger);

impl SampleUniform for IntegerWrapper {
    type Sampler = DashuUniformSampler;
}

impl AddAssign<&'_ IntegerWrapper> for IntegerWrapper {
    fn add_assign(&mut self, rhs: &'_ IntegerWrapper) {
        self.0.add_assign(&rhs.0)
    }
}

pub struct DashuUniformSampler(UniformIBig);

impl UniformSampler for DashuUniformSampler {
    type X = IntegerWrapper;

    fn new<B1, B2>(low: B1, high: B2) -> Self
    where
        B1: rand::distributions::uniform::SampleBorrow<Self::X> + Sized,
        B2: rand::distributions::uniform::SampleBorrow<Self::X> + Sized,
    {
        DashuUniformSampler(UniformIBig::new(&low.borrow().0, &high.borrow().0))
    }

    fn new_inclusive<B1, B2>(low: B1, high: B2) -> Self
    where
        B1: rand::distributions::uniform::SampleBorrow<Self::X> + Sized,
        B2: rand::distributions::uniform::SampleBorrow<Self::X> + Sized,
    {
        DashuUniformSampler(UniformIBig::new_inclusive(
            &low.borrow().0,
            &high.borrow().0,
        ))
    }

    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> Self::X {
        IntegerWrapper(self.0.sample(rng))
    }
}
