awk -v FS="\t" '{A[$2]++} END {for (i in A) print A[i], i}' words_with_pos.txt | sort -nr
