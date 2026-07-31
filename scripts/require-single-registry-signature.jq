if (.signatures | type) == "array" and (.signatures | length) == 1
then
  .signatures[0].key_id
else
  error("Published registry must have exactly one signature before refresh")
end
