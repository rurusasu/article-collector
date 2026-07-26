[
  .[]
  | select(.isDraft == false and .isPrerelease == false)
  | select(.tagName | test("^v[0-9]+\\.[0-9]+\\.[0-9]+$"))
] as $eligible
| if ($eligible | length) == 0 then
    error("No eligible published stable core release")
  elif any($eligible[]; (.publishedAt | type) != "string") then
    error("Eligible core release is missing publishedAt")
  else
    ($eligible | map(.publishedAt) | max) as $latest_published_at
    | [$eligible[] | select(.publishedAt == $latest_published_at)] as $latest
    | if ($latest | length) != 1 then
        error("Latest publishedAt is shared by multiple core releases")
      else
        $latest[0].tagName
      end
  end
