/-! Definitions shared by the challenge and every solution. Trusted. -/

/-- `sumTo n = 0 + 1 + ⋯ + n`. -/
def sumTo : Nat → Nat
  | 0 => 0
  | n + 1 => sumTo n + (n + 1)
