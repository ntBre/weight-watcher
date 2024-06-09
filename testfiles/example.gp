set terminal pngcairo
set output "/tmp/weight-watcher.png"

set timefmt "%Y-%m-%d"
set xdata time
set xrange ["2024-06-01":"2024-06-30"]
set yrange [180:250]
set ylabel "Weight"
set xlabel "Date"
unset key
plot "/home/brent/.config/weight-watcher/weights.dat" u 1:2 w linespoints pointtype 7 lc "black"
