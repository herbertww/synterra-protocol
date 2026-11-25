import Defs

/-! Example target: the sum Gauss found as a schoolboy. Trusted: written by the
nominator. Every submission must prove these exact statements, with Lean's
standard axioms only. -/

theorem gauss (n : Nat) : 2 * sumTo n = n * (n + 1) := sorry

theorem gauss_succ (n : Nat) : 2 * sumTo (n + 1) = (n + 1) * (n + 2) := sorry
