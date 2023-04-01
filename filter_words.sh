awk -v FS="\t" '$0 ~ /^[^\t]+\t(Adjektiv|Adverb|Verb|Substantiv)\t$/ {print $1}' words_with_pos.txt
