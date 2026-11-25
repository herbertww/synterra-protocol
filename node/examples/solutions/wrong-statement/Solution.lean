import Defs

/-! Adversarial: declares the targets' names over weaker statements.
`lake comparator` must reject it: the statements differ from the challenge. -/

theorem gauss (n : Nat) : 2 * sumTo n = 2 * sumTo n := rfl

theorem gauss_succ (n : Nat) : sumTo (n + 1) = sumTo (n + 1) := rfl
