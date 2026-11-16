(function() {
    var dataEl = document.getElementById('exercise-chart-data');
    if (!dataEl) return;
    var raw;
    try { raw = JSON.parse(dataEl.textContent); } catch (e) { return; }
    if (!Array.isArray(raw) || raw.length < 2) return;

    var svg = document.getElementById('exercise-chart');
    var line = document.getElementById('chart-line');
    var dots = document.getElementById('chart-dots');
    var xLabels = document.getElementById('chart-x-labels');
    var yGrid = document.getElementById('chart-y-grid');
    var hitAreas = document.getElementById('chart-hit-areas');
    var tooltip = document.getElementById('exercise-chart-tooltip');
    var wrap = document.getElementById('exercise-chart-wrap');

    var W = 600, H = 220, PL = 44, PR_ = 12, PT = 14, PB = 28;
    var plotW = W - PL - PR_, plotH = H - PT - PB;

    /* Start from the server's selection, not a hardcoded default. */
    var activeMetric = '{{METRIC}}';
    var activeRange = '{{RANGE}}';
    var lastYMin = 0, lastYMax = 1;

    function metricValue(p, m) {
        if (m === 'top_set') return p.top_weight;
        if (m === 'e1rm') return p.e1rm;
        return p.volume;
    }

    function redraw() {
        var slice = activeRange === 'all' ? raw : raw.slice(Math.max(0, raw.length - 20));
        var n = slice.length;
        if (n < 2) {
            // Sparse fallback inside the same SVG: clear line/dots, hide tooltip.
            line.setAttribute('points', '');
            dots.innerHTML = '';
            xLabels.innerHTML = '';
            yGrid.innerHTML = '';
            hitAreas.innerHTML = '';
            tooltip.style.display = 'none';
            return;
        }

        var vals = slice.map(function(p) { return metricValue(p, activeMetric); });
        var mn = Math.min.apply(null, vals);
        var mx = Math.max.apply(null, vals);
        if (mx === mn) { mn -= 1; mx += 1; }
        var pad = (mx - mn) * 0.1;
        var yMin = mn - pad, yMax = mx + pad;
        lastYMin = yMin; lastYMax = yMax;

        // Y grid + labels (4 ticks)
        yGrid.innerHTML = '';
        for (var i = 0; i < 4; i++) {
            var frac = i / 3;
            var y = PT + frac * plotH;
            var v = yMax - frac * (yMax - yMin);
            yGrid.insertAdjacentHTML('beforeend',
                '<line x1="' + PL + '" x2="' + (W - PR_) + '" y1="' + y + '" y2="' + y + '" stroke="var(--border-light)" stroke-width="1"/>' +
                '<text x="' + (PL - 6) + '" y="' + (y + 4) + '" text-anchor="end" font-size="11" fill="var(--text-secondary)">' + v.toFixed(0) + '</text>');
        }

        // Polyline + dots + PR detection
        var pts = [];
        var dotsHtml = '';
        var hitHtml = '';
        var labelHtml = '';
        var running = -Infinity;
        var bandW = plotW / Math.max(1, n - 1);

        for (var j = 0; j < n; j++) {
            var p = slice[j];
            var x = PL + (j / (n - 1)) * plotW;
            var val = metricValue(p, activeMetric);
            var yy = PT + (1 - (val - yMin) / (yMax - yMin)) * plotH;
            pts.push(x.toFixed(2) + ',' + yy.toFixed(2));
            var isPr = val > running;
            if (isPr) running = val;
            var color = isPr ? 'var(--gold)' : 'var(--accent)';
            dotsHtml += '<circle cx="' + x + '" cy="' + yy + '" r="4" fill="' + color + '" stroke="var(--bg-root)" stroke-width="1.5"/>';
            // Invisible hit-area for tooltip
            var hx = j === 0 ? PL : x - bandW / 2;
            var hw = (j === 0 || j === n - 1) ? bandW / 2 : bandW;
            hitHtml += '<rect x="' + hx + '" y="' + PT + '" width="' + hw + '" height="' + plotH + '" fill="transparent" data-i="' + j + '" style="cursor:crosshair"/>';
        }
        line.setAttribute('points', pts.join(' '));
        dots.innerHTML = dotsHtml;
        hitAreas.innerHTML = hitHtml;

        // X labels (up to 5)
        var lc = Math.min(5, n);
        for (var k = 0; k < lc; k++) {
            var idx = Math.floor(k * (n - 1) / Math.max(1, lc - 1));
            var lx = PL + (idx / (n - 1)) * plotW;
            labelHtml += '<text x="' + lx + '" y="' + (H - 8) + '" text-anchor="middle" font-size="11" fill="var(--text-secondary)">' + slice[idx].date.substring(5) + '</text>';
        }
        xLabels.innerHTML = labelHtml;

        // Tooltip handlers
        Array.prototype.forEach.call(hitAreas.querySelectorAll('rect'), function(rect) {
            rect.addEventListener('pointerenter', function(e) { showTip(slice, parseInt(rect.getAttribute('data-i'), 10)); });
            rect.addEventListener('pointermove',  function(e) { showTip(slice, parseInt(rect.getAttribute('data-i'), 10)); });
            rect.addEventListener('pointerleave', function() { tooltip.style.display = 'none'; });
            rect.addEventListener('pointerdown',  function(e) { showTip(slice, parseInt(rect.getAttribute('data-i'), 10)); });
        });
    }

    function showTip(slice, i) {
        var p = slice[i];
        var rect = svg.getBoundingClientRect();
        var n = slice.length;
        var xPct = n === 1 ? 0.5 : i / (n - 1);
        var leftPx = PL + xPct * plotW;
        var leftCss = (leftPx / W) * rect.width;
        // Dot's y in CSS pixels, derived from the active metric's value & current scale.
        var val = metricValue(p, activeMetric);
        var range = lastYMax - lastYMin || 1;
        var dotYSvg = PT + (1 - (val - lastYMin) / range) * plotH;
        var dotYCss = (dotYSvg / H) * rect.height;
        tooltip.innerHTML =
            '<div><strong>' + p.date + '</strong></div>' +
            '<div>Top: ' + p.top_weight + ' kg × ' + p.top_reps + '</div>' +
            '<div>e1RM: ' + p.e1rm.toFixed(1) + ' kg</div>' +
            '<div>Volume: ' + p.volume.toFixed(0) + ' kg</div>';
        tooltip.style.display = 'block';
        // Clamp to wrap; 8px above the dot, or below if that overflows the top.
        var wrapW = wrap.getBoundingClientRect().width;
        var tipW = tooltip.offsetWidth;
        var tipH = tooltip.offsetHeight;
        var left = Math.max(0, Math.min(wrapW - tipW, leftCss - tipW / 2));
        var topAbove = dotYCss - tipH - 8;
        var top = topAbove >= 0 ? topAbove : dotYCss + 8;
        tooltip.style.left = left + 'px';
        tooltip.style.top = top + 'px';
    }

    /* Redraw in place instead of following the tab links. */
    var controls = document.getElementById('exercise-chart-controls');
    controls.addEventListener('click', function(e) {
        if (e.defaultPrevented || e.button !== 0 ||
            e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) {
            /* Modified clicks open the link normally. */
            return;
        }
        var t = e.target.closest ? e.target.closest('[data-metric],[data-range]') : null;
        if (!t || !controls.contains(t)) {
            return;
        }

        e.preventDefault();
        if (t.dataset.metric) {
            activeMetric = t.dataset.metric;
            controls.querySelectorAll('[data-metric]').forEach(function(b) { b.classList.toggle('is-active', b === t); });
        } else {
            activeRange = t.dataset.range;
            controls.querySelectorAll('[data-range]').forEach(function(b) { b.classList.toggle('is-active', b === t); });
        }
        redraw();
        syncLinks();
    });

    /* Point the address bar and every tab href at the state on screen. */
    function syncLinks() {
        var qs = '?metric=' + activeMetric + '&range=' + activeRange;
        if (window.history && window.history.replaceState) {
            window.history.replaceState(null, '', qs);
        }
        controls.querySelectorAll('[data-metric]').forEach(function(a) {
            a.href = '?metric=' + a.dataset.metric + '&range=' + activeRange;
        });
        controls.querySelectorAll('[data-range]').forEach(function(a) {
            a.href = '?metric=' + activeMetric + '&range=' + a.dataset.range;
        });
    }

    redraw();
})();
