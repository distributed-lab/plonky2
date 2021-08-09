use std::convert::TryInto;

use itertools::Itertools;
use num::Integer;

use crate::field::extension_field::target::{ExtensionAlgebraTarget, ExtensionTarget};
use crate::field::extension_field::{Extendable, OEF};
use crate::field::field_types::Field;
use crate::gates::arithmetic::ArithmeticExtensionGate;
use crate::iop::generator::{GeneratedValues, SimpleGenerator};
use crate::iop::target::Target;
use crate::iop::witness::PartialWitness;
use crate::plonk::circuit_builder::CircuitBuilder;
use crate::util::bits_u64;
use crate::with_context;

impl<F: Extendable<D>, const D: usize> CircuitBuilder<F, D> {
    pub fn double_arithmetic_extension(
        &mut self,
        const_0: F,
        const_1: F,
        first_multiplicand_0: ExtensionTarget<D>,
        first_multiplicand_1: ExtensionTarget<D>,
        first_addend: ExtensionTarget<D>,
        second_multiplicand_0: ExtensionTarget<D>,
        second_multiplicand_1: ExtensionTarget<D>,
        second_addend: ExtensionTarget<D>,
    ) -> (ExtensionTarget<D>, ExtensionTarget<D>) {
        let gate = self.add_gate(ArithmeticExtensionGate, vec![const_0, const_1]);

        let wire_first_multiplicand_0 = ExtensionTarget::from_range(
            gate,
            ArithmeticExtensionGate::<D>::wires_first_multiplicand_0(),
        );
        let wire_first_multiplicand_1 = ExtensionTarget::from_range(
            gate,
            ArithmeticExtensionGate::<D>::wires_first_multiplicand_1(),
        );
        let wire_first_addend =
            ExtensionTarget::from_range(gate, ArithmeticExtensionGate::<D>::wires_first_addend());
        let wire_second_multiplicand_0 = ExtensionTarget::from_range(
            gate,
            ArithmeticExtensionGate::<D>::wires_second_multiplicand_0(),
        );
        let wire_second_multiplicand_1 = ExtensionTarget::from_range(
            gate,
            ArithmeticExtensionGate::<D>::wires_second_multiplicand_1(),
        );
        let wire_second_addend =
            ExtensionTarget::from_range(gate, ArithmeticExtensionGate::<D>::wires_second_addend());
        let wire_first_output =
            ExtensionTarget::from_range(gate, ArithmeticExtensionGate::<D>::wires_first_output());
        let wire_second_output =
            ExtensionTarget::from_range(gate, ArithmeticExtensionGate::<D>::wires_second_output());

        self.route_extension(first_multiplicand_0, wire_first_multiplicand_0);
        self.route_extension(first_multiplicand_1, wire_first_multiplicand_1);
        self.route_extension(first_addend, wire_first_addend);
        self.route_extension(second_multiplicand_0, wire_second_multiplicand_0);
        self.route_extension(second_multiplicand_1, wire_second_multiplicand_1);
        self.route_extension(second_addend, wire_second_addend);
        (wire_first_output, wire_second_output)
    }

    pub fn arithmetic_extension(
        &mut self,
        const_0: F,
        const_1: F,
        multiplicand_0: ExtensionTarget<D>,
        multiplicand_1: ExtensionTarget<D>,
        addend: ExtensionTarget<D>,
    ) -> ExtensionTarget<D> {
        // See if we can determine the result without adding an `ArithmeticGate`.
        if let Some(result) = self.arithmetic_extension_special_cases(
            const_0,
            const_1,
            multiplicand_0,
            multiplicand_1,
            addend,
        ) {
            return result;
        }

        let zero = self.zero_extension();
        self.double_arithmetic_extension(
            const_0,
            const_1,
            multiplicand_0,
            multiplicand_1,
            addend,
            zero,
            zero,
            zero,
        )
        .0
    }

    /// Checks for special cases where the value of
    /// `const_0 * multiplicand_0 * multiplicand_1 + const_1 * addend`
    /// can be determined without adding an `ArithmeticGate`.
    fn arithmetic_extension_special_cases(
        &mut self,
        const_0: F,
        const_1: F,
        multiplicand_0: ExtensionTarget<D>,
        multiplicand_1: ExtensionTarget<D>,
        addend: ExtensionTarget<D>,
    ) -> Option<ExtensionTarget<D>> {
        let zero = self.zero_extension();

        let mul_0_const = self.target_as_constant_ext(multiplicand_0);
        let mul_1_const = self.target_as_constant_ext(multiplicand_1);
        let addend_const = self.target_as_constant_ext(addend);

        let first_term_zero =
            const_0 == F::ZERO || multiplicand_0 == zero || multiplicand_1 == zero;
        let second_term_zero = const_1 == F::ZERO || addend == zero;

        // If both terms are constant, return their (constant) sum.
        let first_term_const = if first_term_zero {
            Some(F::Extension::ZERO)
        } else if let (Some(x), Some(y)) = (mul_0_const, mul_1_const) {
            Some(x * y * const_0.into())
        } else {
            None
        };
        let second_term_const = if second_term_zero {
            Some(F::Extension::ZERO)
        } else {
            addend_const.map(|x| x * const_1.into())
        };
        if let (Some(x), Some(y)) = (first_term_const, second_term_const) {
            return Some(self.constant_extension(x + y));
        }

        if first_term_zero && const_1.is_one() {
            return Some(addend);
        }

        if second_term_zero {
            if let Some(x) = mul_0_const {
                if (x * const_0.into()).is_one() {
                    return Some(multiplicand_1);
                }
            }
            if let Some(x) = mul_1_const {
                if (x * const_0.into()).is_one() {
                    return Some(multiplicand_0);
                }
            }
        }

        None
    }

    pub fn arithmetic_many_extension(
        &mut self,
        const_0: F,
        const_1: F,
        operands: Vec<[ExtensionTarget<D>; 3]>,
    ) -> Vec<ExtensionTarget<D>> {
        let mut res = Vec::new();
        for chunk in operands.chunks_exact(2) {
            let [fm0, fm1, fa] = chunk[0];
            let [sm0, sm1, sa] = chunk[1];
            let arithm =
                self.double_arithmetic_extension(const_0, const_1, fm0, fm1, fa, sm0, sm1, sa);
            res.push(arithm.0);
            res.push(arithm.1);
        }
        if operands.len().is_odd() {
            let [m0, m1, a] = operands[operands.len() - 1];
            res.push(self.arithmetic_extension(const_0, const_1, m0, m1, a));
        }

        res
    }

    pub fn add_extension(
        &mut self,
        a: ExtensionTarget<D>,
        b: ExtensionTarget<D>,
    ) -> ExtensionTarget<D> {
        let one = self.one_extension();
        self.arithmetic_extension(F::ONE, F::ONE, one, a, b)
    }

    /// Returns `(a0+b0, a1+b1)`.
    pub fn add_two_extension(
        &mut self,
        a0: ExtensionTarget<D>,
        b0: ExtensionTarget<D>,
        a1: ExtensionTarget<D>,
        b1: ExtensionTarget<D>,
    ) -> (ExtensionTarget<D>, ExtensionTarget<D>) {
        let one = self.one_extension();
        self.double_arithmetic_extension(F::ONE, F::ONE, one, a0, b0, one, a1, b1)
    }

    pub fn add_ext_algebra(
        &mut self,
        a: ExtensionAlgebraTarget<D>,
        b: ExtensionAlgebraTarget<D>,
    ) -> ExtensionAlgebraTarget<D> {
        // We run two additions in parallel. So `[a0,a1,a2,a3] + [b0,b1,b2,b3]` is computed with two
        // `add_two_extension`, first `[a0,a1]+[b0,b1]` then `[a2,a3]+[b2,b3]`.
        let mut res = Vec::with_capacity(D);
        // We need some extra logic if D is odd.
        let d_even = D & (D ^ 1); // = 2 * (D/2)
        for mut chunk in &(0..d_even).chunks(2) {
            let i = chunk.next().unwrap();
            let j = chunk.next().unwrap();
            let (o0, o1) = self.add_two_extension(a.0[i], b.0[i], a.0[j], b.0[j]);
            res.extend([o0, o1]);
        }
        if D.is_odd() {
            res.push(self.add_extension(a.0[D - 1], b.0[D - 1]));
        }
        ExtensionAlgebraTarget(res.try_into().unwrap())
    }

    /// Add 3 `ExtensionTarget`s with 1 `ArithmeticExtensionGate`s.
    pub fn add_three_extension(
        &mut self,
        a: ExtensionTarget<D>,
        b: ExtensionTarget<D>,
        c: ExtensionTarget<D>,
    ) -> ExtensionTarget<D> {
        let one = self.one_extension();
        let gate = self.num_gates();
        let first_out =
            ExtensionTarget::from_range(gate, ArithmeticExtensionGate::<D>::wires_first_output());
        self.double_arithmetic_extension(F::ONE, F::ONE, one, a, b, one, c, first_out)
            .1
    }

    /// Add `n` `ExtensionTarget`s with `n/2` `ArithmeticExtensionGate`s.
    pub fn add_many_extension(&mut self, terms: &[ExtensionTarget<D>]) -> ExtensionTarget<D> {
        let zero = self.zero_extension();
        let mut terms = terms.to_vec();
        if terms.is_empty() {
            return zero;
        } else if terms.len() < 3 {
            terms.resize(3, zero);
        } else if terms.len().is_even() {
            terms.push(zero);
        }

        let mut acc = self.add_three_extension(terms[0], terms[1], terms[2]);
        terms.drain(0..3);
        for chunk in terms.chunks_exact(2) {
            acc = self.add_three_extension(acc, chunk[0], chunk[1]);
        }
        acc
    }

    pub fn sub_extension(
        &mut self,
        a: ExtensionTarget<D>,
        b: ExtensionTarget<D>,
    ) -> ExtensionTarget<D> {
        let one = self.one_extension();
        self.arithmetic_extension(F::ONE, F::NEG_ONE, one, a, b)
    }

    pub fn sub_two_extension(
        &mut self,
        a0: ExtensionTarget<D>,
        b0: ExtensionTarget<D>,
        a1: ExtensionTarget<D>,
        b1: ExtensionTarget<D>,
    ) -> (ExtensionTarget<D>, ExtensionTarget<D>) {
        let one = self.one_extension();
        self.double_arithmetic_extension(F::ONE, F::NEG_ONE, one, a0, b0, one, a1, b1)
    }

    pub fn sub_ext_algebra(
        &mut self,
        a: ExtensionAlgebraTarget<D>,
        b: ExtensionAlgebraTarget<D>,
    ) -> ExtensionAlgebraTarget<D> {
        // See `add_ext_algebra`.
        let mut res = Vec::with_capacity(D);
        let d_even = D & (D ^ 1); // = 2 * (D/2)
        for mut chunk in &(0..d_even).chunks(2) {
            let i = chunk.next().unwrap();
            let j = chunk.next().unwrap();
            let (o0, o1) = self.sub_two_extension(a.0[i], b.0[i], a.0[j], b.0[j]);
            res.extend([o0, o1]);
        }
        if D.is_odd() {
            res.push(self.sub_extension(a.0[D - 1], b.0[D - 1]));
        }
        ExtensionAlgebraTarget(res.try_into().unwrap())
    }

    pub fn mul_extension_with_const(
        &mut self,
        const_0: F,
        multiplicand_0: ExtensionTarget<D>,
        multiplicand_1: ExtensionTarget<D>,
    ) -> ExtensionTarget<D> {
        let zero = self.zero_extension();
        self.double_arithmetic_extension(
            const_0,
            F::ZERO,
            multiplicand_0,
            multiplicand_1,
            zero,
            zero,
            zero,
            zero,
        )
        .0
    }

    pub fn mul_extension(
        &mut self,
        multiplicand_0: ExtensionTarget<D>,
        multiplicand_1: ExtensionTarget<D>,
    ) -> ExtensionTarget<D> {
        self.mul_extension_with_const(F::ONE, multiplicand_0, multiplicand_1)
    }

    /// Returns `(a0*b0, a1*b1)`.
    pub fn mul_two_extension(
        &mut self,
        a0: ExtensionTarget<D>,
        b0: ExtensionTarget<D>,
        a1: ExtensionTarget<D>,
        b1: ExtensionTarget<D>,
    ) -> (ExtensionTarget<D>, ExtensionTarget<D>) {
        let zero = self.zero_extension();
        self.double_arithmetic_extension(F::ONE, F::ZERO, a0, b0, zero, a1, b1, zero)
    }

    /// Computes `x^2`.
    pub fn square_extension(&mut self, x: ExtensionTarget<D>) -> ExtensionTarget<D> {
        self.mul_extension(x, x)
    }

    /// Computes `x^3`.
    pub fn cube_extension(&mut self, x: ExtensionTarget<D>) -> ExtensionTarget<D> {
        self.mul_three_extension(x, x, x)
    }

    pub fn mul_ext_algebra(
        &mut self,
        a: ExtensionAlgebraTarget<D>,
        b: ExtensionAlgebraTarget<D>,
    ) -> ExtensionAlgebraTarget<D> {
        let zero = self.zero_extension();
        let mut ops = Vec::new();
        let mut opsw = Vec::new();
        for i in 0..D {
            for j in 0..D - i {
                ops.push([a.0[i], b.0[j], zero]);
            }
            for j in D - i..D {
                opsw.push([a.0[i], b.0[j], zero]);
            }
        }
        let mut muls = self.arithmetic_many_extension(F::ONE, F::ONE, ops);
        let mut mulsw = self.arithmetic_many_extension(F::Extension::W, F::ONE, opsw);
        let mut toadd = vec![vec![]; D];
        for i in 0..D {
            for j in 0..D - i {
                toadd[(i + j) % D].push(muls.remove(0));
            }
            for j in D - i..D {
                toadd[(i + j) % D].push(mulsw.remove(0));
            }
        }
        let mut res = [zero; D];
        for i in 0..D {
            res[i] = self.add_many_extension(&toadd[i]);
        }
        ExtensionAlgebraTarget(res)
    }

    /// Multiply 3 `ExtensionTarget`s with 1 `ArithmeticExtensionGate`s.
    pub fn mul_three_extension(
        &mut self,
        a: ExtensionTarget<D>,
        b: ExtensionTarget<D>,
        c: ExtensionTarget<D>,
    ) -> ExtensionTarget<D> {
        let zero = self.zero_extension();
        let gate = self.num_gates();
        let first_out =
            ExtensionTarget::from_range(gate, ArithmeticExtensionGate::<D>::wires_first_output());
        self.double_arithmetic_extension(F::ONE, F::ZERO, a, b, zero, c, first_out, zero)
            .1
    }

    /// Multiply `n` `ExtensionTarget`s with `n/2` `ArithmeticExtensionGate`s.
    pub fn mul_many_extension(&mut self, terms: &[ExtensionTarget<D>]) -> ExtensionTarget<D> {
        let one = self.one_extension();
        let mut terms = terms.to_vec();
        if terms.is_empty() {
            return one;
        } else if terms.len() < 3 {
            terms.resize(3, one);
        } else if terms.len().is_even() {
            terms.push(one);
        }
        let mut acc = self.mul_three_extension(terms[0], terms[1], terms[2]);
        terms.drain(0..3);
        for chunk in terms.chunks_exact(2) {
            acc = self.mul_three_extension(acc, chunk[0], chunk[1]);
        }
        acc
    }

    /// Like `mul_add`, but for `ExtensionTarget`s.
    pub fn mul_add_extension(
        &mut self,
        a: ExtensionTarget<D>,
        b: ExtensionTarget<D>,
        c: ExtensionTarget<D>,
    ) -> ExtensionTarget<D> {
        self.arithmetic_extension(F::ONE, F::ONE, a, b, c)
    }

    /// Like `mul_add`, but for `ExtensionTarget`s.
    pub fn scalar_mul_add_extension(
        &mut self,
        a: Target,
        b: ExtensionTarget<D>,
        c: ExtensionTarget<D>,
    ) -> ExtensionTarget<D> {
        let a_ext = self.convert_to_ext(a);
        self.arithmetic_extension(F::ONE, F::ONE, a_ext, b, c)
    }

    /// Like `mul_sub`, but for `ExtensionTarget`s.
    pub fn mul_sub_extension(
        &mut self,
        a: ExtensionTarget<D>,
        b: ExtensionTarget<D>,
        c: ExtensionTarget<D>,
    ) -> ExtensionTarget<D> {
        self.arithmetic_extension(F::ONE, F::NEG_ONE, a, b, c)
    }

    /// Like `mul_sub`, but for `ExtensionTarget`s.
    pub fn scalar_mul_sub_extension(
        &mut self,
        a: Target,
        b: ExtensionTarget<D>,
        c: ExtensionTarget<D>,
    ) -> ExtensionTarget<D> {
        let a_ext = self.convert_to_ext(a);
        self.arithmetic_extension(F::ONE, F::NEG_ONE, a_ext, b, c)
    }

    /// Returns `a * b`, where `b` is in the extension field and `a` is in the base field.
    pub fn scalar_mul_ext(&mut self, a: Target, b: ExtensionTarget<D>) -> ExtensionTarget<D> {
        let a_ext = self.convert_to_ext(a);
        self.mul_extension(a_ext, b)
    }

    /// Returns `a * b`, where `b` is in the extension of the extension field, and `a` is in the
    /// extension field.
    pub fn scalar_mul_ext_algebra(
        &mut self,
        a: ExtensionTarget<D>,
        mut b: ExtensionAlgebraTarget<D>,
    ) -> ExtensionAlgebraTarget<D> {
        for i in 0..D {
            b.0[i] = self.mul_extension(a, b.0[i]);
        }
        b
    }

    /// Exponentiate `base` to the power of `2^power_log`.
    // TODO: Test
    pub fn exp_power_of_2_extension(
        &mut self,
        mut base: ExtensionTarget<D>,
        power_log: usize,
    ) -> ExtensionTarget<D> {
        for _ in 0..power_log {
            base = self.square_extension(base);
        }
        base
    }

    /// Exponentiate `base` to the power of a known `exponent`.
    // TODO: Test
    pub fn exp_u64_extension(
        &mut self,
        base: ExtensionTarget<D>,
        exponent: u64,
    ) -> ExtensionTarget<D> {
        match exponent {
            0 => return self.one_extension(),
            1 => return base,
            2 => return self.square_extension(base),
            3 => return self.cube_extension(base),
            _ => (),
        }
        let mut current = base;
        let mut product = self.one_extension();

        for j in 0..bits_u64(exponent as u64) {
            if (exponent >> j & 1) != 0 {
                product = self.mul_extension(product, current);
            }
            current = self.square_extension(current);
        }
        product
    }

    /// Computes `x / y`. Results in an unsatisfiable instance if `y = 0`.
    pub fn div_extension(
        &mut self,
        x: ExtensionTarget<D>,
        y: ExtensionTarget<D>,
    ) -> ExtensionTarget<D> {
        let y_inv = self.inverse_extension(y);
        self.mul_extension(x, y_inv)
    }

    /// Computes `q = x / y` by witnessing `q` and requiring that `q * y = x`. This can be unsafe in
    /// some cases, as it allows `0 / 0 = <anything>`.
    pub fn div_unsafe_extension(
        &mut self,
        x: ExtensionTarget<D>,
        y: ExtensionTarget<D>,
    ) -> ExtensionTarget<D> {
        let quotient = self.add_virtual_extension_target();
        self.add_generator(QuotientGeneratorExtension {
            numerator: x,
            denominator: y,
            quotient,
        });

        // Enforce that q y = x.
        let q_y = self.mul_extension(quotient, y);
        self.assert_equal_extension(q_y, x);

        quotient
    }

    /// Computes `1 / x`. Results in an unsatisfiable instance if `x = 0`.
    pub fn inverse_extension(&mut self, x: ExtensionTarget<D>) -> ExtensionTarget<D> {
        let inv = self.add_virtual_extension_target();
        let one = self.one_extension();
        self.add_generator(QuotientGeneratorExtension {
            numerator: one,
            denominator: x,
            quotient: inv,
        });

        // Enforce that x times its purported inverse equals 1.
        let x_inv = self.mul_extension(x, inv);
        self.assert_equal_extension(x_inv, one);

        inv
    }
}

struct QuotientGeneratorExtension<const D: usize> {
    numerator: ExtensionTarget<D>,
    denominator: ExtensionTarget<D>,
    quotient: ExtensionTarget<D>,
}

impl<F: Extendable<D>, const D: usize> SimpleGenerator<F> for QuotientGeneratorExtension<D> {
    fn dependencies(&self) -> Vec<Target> {
        let mut deps = self.numerator.to_target_array().to_vec();
        deps.extend(&self.denominator.to_target_array());
        deps
    }

    fn run_once(&self, witness: &PartialWitness<F>, out_buffer: &mut GeneratedValues<F>) {
        let num = witness.get_extension_target(self.numerator);
        let dem = witness.get_extension_target(self.denominator);
        let quotient = num / dem;
        out_buffer.set_extension_target(self.quotient, quotient)
    }
}

/// An iterator over the powers of a certain base element `b`: `b^0, b^1, b^2, ...`.
#[derive(Clone)]
pub struct PowersTarget<const D: usize> {
    base: ExtensionTarget<D>,
    current: ExtensionTarget<D>,
}

impl<const D: usize> PowersTarget<D> {
    pub fn next<F: Extendable<D>>(
        &mut self,
        builder: &mut CircuitBuilder<F, D>,
    ) -> ExtensionTarget<D> {
        let result = self.current;
        self.current = builder.mul_extension(self.base, self.current);
        result
    }

    pub fn repeated_frobenius<F: Extendable<D>>(
        self,
        k: usize,
        builder: &mut CircuitBuilder<F, D>,
    ) -> Self {
        let Self { base, current } = self;
        Self {
            base: base.repeated_frobenius(k, builder),
            current: current.repeated_frobenius(k, builder),
        }
    }
}

impl<F: Extendable<D>, const D: usize> CircuitBuilder<F, D> {
    pub fn powers(&mut self, base: ExtensionTarget<D>) -> PowersTarget<D> {
        PowersTarget {
            base,
            current: self.one_extension(),
        }
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;

    use crate::field::crandall_field::CrandallField;
    use crate::field::extension_field::quartic::QuarticCrandallField;
    use crate::field::field_types::Field;
    use crate::iop::witness::PartialWitness;
    use crate::plonk::circuit_builder::CircuitBuilder;
    use crate::plonk::circuit_data::CircuitConfig;
    use crate::plonk::verifier::verify;

    #[test]
    fn test_mul_many() -> Result<()> {
        type F = CrandallField;
        type FF = QuarticCrandallField;
        const D: usize = 4;

        let config = CircuitConfig::large_config();

        let mut builder = CircuitBuilder::<F, D>::new(config);
        let mut pw = PartialWitness::new();

        let vs = FF::rand_vec(3);
        let ts = builder.add_virtual_extension_targets(3);
        for (&v, &t) in vs.iter().zip(&ts) {
            pw.set_extension_target(t, v);
        }
        let mul0 = builder.mul_many_extension(&ts);
        let mul1 = {
            let mut acc = builder.one_extension();
            for &t in &ts {
                acc = builder.mul_extension(acc, t);
            }
            acc
        };
        let mul2 = builder.mul_three_extension(ts[0], ts[1], ts[2]);
        let mul3 = builder.constant_extension(vs.into_iter().product());

        builder.assert_equal_extension(mul0, mul1);
        builder.assert_equal_extension(mul1, mul2);
        builder.assert_equal_extension(mul2, mul3);

        let data = builder.build();
        let proof = data.prove(pw)?;

        verify(proof, &data.verifier_only, &data.common)
    }

    #[test]
    fn test_div_extension() -> Result<()> {
        type F = CrandallField;
        type FF = QuarticCrandallField;
        const D: usize = 4;

        let config = CircuitConfig::large_config();

        let mut builder = CircuitBuilder::<F, D>::new(config);

        let x = FF::rand();
        let y = FF::rand();
        let z = x / y;
        let xt = builder.constant_extension(x);
        let yt = builder.constant_extension(y);
        let zt = builder.constant_extension(z);
        let comp_zt = builder.div_extension(xt, yt);
        let comp_zt_unsafe = builder.div_unsafe_extension(xt, yt);
        builder.assert_equal_extension(zt, comp_zt);
        builder.assert_equal_extension(zt, comp_zt_unsafe);

        let data = builder.build();
        let proof = data.prove(PartialWitness::new())?;

        verify(proof, &data.verifier_only, &data.common)
    }
}
