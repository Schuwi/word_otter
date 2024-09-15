use std::ops::AddAssign;

use dashu::integer::rand::UniformIBig;
use rand::{distributions::uniform::{SampleUniform, UniformSampler}, Rng};

pub type BigInteger = dashu::Integer;

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
        DashuUniformSampler(UniformIBig::new_inclusive(&low.borrow().0, &high.borrow().0))
    }

    
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> Self::X {
        IntegerWrapper(self.0.sample(rng))
    }
}