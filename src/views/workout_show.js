
var lastWeightInfoEl = document.getElementById('exercise-last-weight-info');
var exerciseSelect = document.getElementById('exercise_id');

function showLastWeightInfo(exerciseId) {
    if (exerciseId && exerciseLastWeights[exerciseId]) {
        var entry = exerciseLastWeights[exerciseId];
        var rpeHtml = (entry.rpe !== null)
            ? '<span class="rpe-chip">RPE ' + entry.rpe + '</span>'
            : '<span class="rpe-chip rpe-chip-empty">RPE —</span>';
        var loggedDate = window.LiftLog.formatLocalDate(entry.loggedAt) || '';
        lastWeightInfoEl.innerHTML = "Last: " + entry.weight + " kg " + rpeHtml +
            ' <span class="muted">' + loggedDate + '</span>' +
            ' <button type="button" class="btn btn-sm btn-inline" style="background:var(--gold);color:var(--text-inverse);border-color:var(--gold);" onclick="fillLastWeight()">Fill</button>';
        lastWeightInfoEl.classList.add('visible');
    } else {
        lastWeightInfoEl.innerHTML = "";
        lastWeightInfoEl.classList.remove('visible');
    }
}

function fillLastWeight() {
    var exerciseId = exerciseSelect.value;
    if (exerciseId && exerciseLastWeights[exerciseId]) {
        document.getElementById('weight').value = exerciseLastWeights[exerciseId].weight;
    }
}

exerciseSelect.addEventListener('change', function() {
    showLastWeightInfo(this.value);
});

function cloneSet(exerciseId, weight, reps, rpe) {
    exerciseSelect.value = exerciseId;
    document.getElementById('weight').value = weight;
    document.getElementById('reps').value = reps;
    document.getElementById('rpe').value = rpe === null ? '' : rpe;
    showLastWeightInfo(exerciseId);
    exerciseSelect.scrollIntoView({ behavior: 'smooth' });
}

/* Fill the form in place instead of following the `?prefill=` link;
   modified clicks still open it. */
document.addEventListener('click', function (e) {
    if (e.defaultPrevented || e.button !== 0 ||
        e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) {
        return;
    }
    var target = e.target;
    if (!target || typeof target.closest !== 'function') {
        return;
    }
    var link = target.closest('a[data-clone-exercise]');
    if (!link) {
        return;
    }

    e.preventDefault();
    var rpe = link.dataset.cloneRpe;
    cloneSet(
        link.dataset.cloneExercise,
        link.dataset.cloneWeight,
        link.dataset.cloneReps,
        rpe === '' ? null : rpe
    );
});

/* Server-prefilled form (?prefill=…): show its hint too. */
if (exerciseSelect.value) {
    showLastWeightInfo(exerciseSelect.value);
}

document.addEventListener('click', function (e) {
    var target = e.target;
    if (!target || typeof target.closest !== 'function') {
        return;
    }
    var btn = target.closest('[data-copy-url]');
    if (!btn) {
        return;
    }

    var fullUrl = window.location.origin + btn.dataset.copyUrl;
    navigator.clipboard.writeText(fullUrl).then(function() {
        alert('Link copied to clipboard!');
    }, function() {
        prompt('Copy this link:', fullUrl);
    });
});
