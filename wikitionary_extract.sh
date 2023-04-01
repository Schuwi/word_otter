xmlstarlet sel -t -m '/_:mediawiki/_:page[position() < 10000]' -i '_:ns = 0' -v '_:revision/_:text' -o "schuwi:record-break" dewiktionary-20230320-pages-meta-current.xml \
| gawk -v FS="schuwi:field-break" -v RS="schuwi:record-break" -v ORS="\n" -v OFS="\t" \
'match($0, "== ([a-zA-Z]+) \\({{Sprache\\|Deutsch}}\\) ==\n=== ({{([^}]+)}}[, ]{0,2})({{([^}]+)}}[, ]{0,2})?({{([^}]+)}}[, ]{0,2})?({{([^}]+)}}[, ]{0,2})?({{([^}]+)}}[, ]{0,2})?({{([^}]+)}}[, ]{0,2})? ===", m) {print m[1], m[3], m[5], m[7], m[9], m[11], m[13]}' \
| gawk -v FS="\t" '{printf("%s\t", $1); for (i=2; i<=NF; i++) {if (match($i, "Wortart\\|([^|]+)\\|Deutsch", m)) printf("%s\t", m[1])}; print ""}'
