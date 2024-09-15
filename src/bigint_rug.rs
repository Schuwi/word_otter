use std::ops::AddAssign;

use rand::{distributions::uniform::{SampleUniform, UniformSampler}, Rng};

pub type BigInteger = rug::Integer;

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