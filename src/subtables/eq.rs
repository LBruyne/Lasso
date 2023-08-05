use ark_ff::PrimeField;
use ark_std::log2;

use crate::utils::{pack_field_xyz, split_bits};

use super::SubtableStrategy;

pub enum EqSubtableStrategy {}

impl<F: PrimeField, const C: usize, const M: usize> SubtableStrategy<F, C, M>
  for EqSubtableStrategy
{
  const NUM_SUBTABLES: usize = 1;
  const NUM_MEMORIES: usize = C;

  fn materialize_subtables() -> [Vec<F>; <Self as SubtableStrategy<F, C, M>>::NUM_SUBTABLES] {
    let mut materialized: Vec<F> = Vec::with_capacity(M);
    let bits_per_operand = (log2(M) / 2) as usize;

    // Materialize table in counting order where lhs | rhs counts 0->m
    for idx in 0..M {
      let (lhs, rhs) = split_bits(idx, bits_per_operand);
      let out = (lhs == rhs).into();

      // Note packs memory T[row] = lhs | rhs | out
      let row = pack_field_xyz(lhs, rhs, out, bits_per_operand);
      materialized.push(row);
    }

    [materialized]
  }

  fn evaluate_subtable_mle(_: usize, point: &Vec<F>) -> F {
    debug_assert!(point.len() % 2 == 0);
    let b = point.len() / 2;
    let (x, y) = point.split_at(b);

    let mut result = F::zero();
    let mut eq_term = F::one();
    for i in 0..b {
      eq_term *=
        x[b - i - 1] * y[b - i - 1] + (F::one() - x[b - i - 1]) * (F::one() - y[b - i - 1]);
      result += F::from(1u64 << (b + i)) * y[b - i - 1];
      result += F::from(1u64 << (2 * b + i)) * x[b - i - 1];
    }
    result += eq_term;
    result
  }

  /// Combine EQ table subtable evaluations
  /// T = T'[0] + 2^16*T'[1] + 2^32*T'[2] + 2^48*T'[3]
  /// T'[3] | T'[2] | T'[1] | T'[0]
  /// x3 | y3 | z3 | x2 | y2 | z2 | x1 | y1 | z1 | x0 | y0 | z0 |
  fn combine_lookups(vals: &[F; <Self as SubtableStrategy<F, C, M>>::NUM_MEMORIES]) -> F {
    let increment = log2(M) as usize;
    let mut sum = F::zero();
    for i in 0..C {
      let weight: u64 = 1u64 << (i * increment);
      sum += F::from(weight) * vals[i];
    }
    sum
  }

  fn g_poly_degree() -> usize {
    1
  }
}

#[cfg(test)]
mod test {
  use crate::{
    materialization_mle_parity_test, subtables::Subtables, utils::index_to_field_bitvector,
  };

  use super::*;
  use ark_curve25519::Fr;
  use ark_ff::Zero;

  #[test]
  fn table_materialization_hardcoded() {
    const C: usize = 4;
    const M: usize = 1 << 4;

    let materialized: [Vec<Fr>; 1] =
      <EqSubtableStrategy as SubtableStrategy<Fr, C, M>>::materialize_subtables();
    assert_eq!(materialized.len(), 1);
    assert_eq!(materialized[0].len(), M);

    let table: Vec<Fr> = materialized[0].clone();
    assert_eq!(table[0], Fr::from(0b00_00_01));
    assert_eq!(table[1], Fr::from(0b00_01_00));
    assert_eq!(table[2], Fr::from(0b00_10_00));
    assert_eq!(table[3], Fr::from(0b00_11_00));
    assert_eq!(table[4], Fr::from(0b01_00_00));
    assert_eq!(table[5], Fr::from(0b01_01_01));
    assert_eq!(table[6], Fr::from(0b01_10_00));
    assert_eq!(table[7], Fr::from(0b01_11_00));
    assert_eq!(table[8], Fr::from(0b10_00_00));
    assert_eq!(table[9], Fr::from(0b10_01_00));
    assert_eq!(table[10], Fr::from(0b10_10_01));
    // ...
  }

  #[test]
  fn combine() {
    const M: usize = 1 << 16;
    let combined: Fr = <EqSubtableStrategy as SubtableStrategy<Fr, 4, M>>::combine_lookups(&[
      Fr::from(100),
      Fr::from(200),
      Fr::from(300),
      Fr::from(400),
    ]);

    // 2^0 * 100 + 2^16 * 200 + 2^32 * 300 + 2^48 * 400
    let expected = (1u64 * 100u64)
      + ((1u64 << 16u64) * 200u64)
      + ((1u64 << 32u64) * 300u64)
      + ((1u64 << 48u64) * 400u64);
    assert_eq!(combined, Fr::from(expected));
  }

  #[test]
  fn valid_merged_poly() {
    const C: usize = 2;
    const M: usize = 1 << 4;

    let x_indices: Vec<usize> = vec![0, 2];
    let y_indices: Vec<usize> = vec![5, 9];

    let subtable_evals: Subtables<Fr, C, M, EqSubtableStrategy> =
      Subtables::new(&[x_indices, y_indices], 2);

    // Real equation here is log2(sparsity) + log2(C)
    let combined_table_index_bits = 2;

    for (x, expected) in vec![
      (0, 00_00_01),   // eq(0) -> 00 & 00 = 00 -> 00_00_01
      (1, 0b00_10_00), // eq(2) -> 00 & 10 = 00 -> 00_10_00
      (2, 0b01_01_01), // eq(5) -> 01 & 01 = 01 -> 01_01_01
      (3, 0b10_01_00), // eq(9)  -> 10 & 01 = 00 -> 10_01_00
    ] {
      let calculated = subtable_evals
        .combined_poly
        .evaluate(&index_to_field_bitvector(x, combined_table_index_bits));
      assert_eq!(calculated, Fr::from(expected));
    }
  }

  materialization_mle_parity_test!(materialization_parity, EqSubtableStrategy, Fr, 16, 1);
  materialization_mle_parity_test!(
    materialization_parity_nonzero_c,
    EqSubtableStrategy,
    Fr,
    16,
    2
  );
}
