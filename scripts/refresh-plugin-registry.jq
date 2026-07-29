if (.revision | type) != "number"
  or .revision < 0
  or .revision != (.revision | floor)
then
  error("Published registry revision must be a non-negative integer")
else
  .revision = $revision
  | .generated_at = $generated_at
  | .expires_at = $expires_at
end
