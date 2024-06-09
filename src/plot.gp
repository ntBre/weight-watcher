set terminal pngcairo
set output "/tmp/weight-watcher.png"

set timefmt "%Y-%m-%d"
set xdata time
set xrange ["{{date_start}}":"{{date_end}}"]
{{yrange}}
set ylabel "Weight"
set xlabel "Date"
unset key
plot "{{name}}" u 1:2 w linespoints pointtype 7 lc "black"