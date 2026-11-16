        /* Rewrite server-rendered UTC timestamps into the browser's timezone.
           Date getters, not toLocaleString(), keep the width constant. */
        (function () {
            function pad(n) {
                return n < 10 ? '0' + n : '' + n;
            }

            /* "GMT+8" / "GMT-3:30", matching Intl's shortOffset spelling. */
            function tzOffsetLabel(d) {
                var mins = -d.getTimezoneOffset();
                var sign = mins < 0 ? '-' : '+';
                var abs = Math.abs(mins);
                var h = Math.floor(abs / 60);
                var m = abs % 60;
                return 'GMT' + sign + h + (m ? ':' + pad(m) : '');
            }

            function tzLabel(d) {
                try {
                    var parts = new Intl.DateTimeFormat(undefined, { timeZoneName: 'shortOffset' })
                        .formatToParts(d);
                    for (var i = 0; i < parts.length; i++) {
                        if (parts[i].type === 'timeZoneName') {
                            return parts[i].value;
                        }
                    }
                } catch (e) {
                    /* shortOffset unsupported — fall through. */
                }
                return tzOffsetLabel(d);
            }

            function parse(iso) {
                var d = new Date(iso);
                return isNaN(d.getTime()) ? null : d;
            }

            function formatLocalDate(iso) {
                var d = parse(iso);
                if (!d) {
                    return null;
                }
                return d.getFullYear() + '-' + pad(d.getMonth() + 1) + '-' + pad(d.getDate());
            }

            function formatLocalDateTime(iso) {
                var d = parse(iso);
                if (!d) {
                    return null;
                }
                return formatLocalDate(iso) + ' ' + pad(d.getHours()) + ':' + pad(d.getMinutes()) +
                    ' ' + tzLabel(d);
            }

            window.LiftLog = window.LiftLog || {};
            window.LiftLog.formatLocalDate = formatLocalDate;
            window.LiftLog.formatLocalDateTime = formatLocalDateTime;

            document.addEventListener('DOMContentLoaded', function () {
                var nodes = document.querySelectorAll('time[datetime][data-fmt]');
                for (var i = 0; i < nodes.length; i++) {
                    var el = nodes[i];
                    var text = el.dataset.fmt === 'date'
                        ? formatLocalDate(el.getAttribute('datetime'))
                        : formatLocalDateTime(el.getAttribute('datetime'));
                    if (text) {
                        el.textContent = text;
                    }
                }
            });
        })();

        /* Reveal script-only controls, which ship `hidden`. */
        (function () {
            document.addEventListener('DOMContentLoaded', function () {
                var nodes = document.querySelectorAll('[data-requires-js]');
                for (var i = 0; i < nodes.length; i++) {
                    nodes[i].hidden = false;
                }
            });
        })();

        /* `a[data-confirm]` links go to a server confirmation page; with
           scripts, ask in a dialog and POST to the same URL instead. */
        (function () {
            document.addEventListener('click', function (e) {
                if (e.defaultPrevented || e.button !== 0 ||
                    e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) {
                    /* Modified clicks open the page as a normal link. */
                    return;
                }
                var target = e.target;
                if (!target || typeof target.closest !== 'function') {
                    return;
                }
                var link = target.closest('a[data-confirm]');
                if (!link) {
                    return;
                }

                e.preventDefault();
                if (!window.confirm(link.dataset.confirm)) {
                    return;
                }

                /* A navigating form POST, so the redirect is followed. */
                var form = document.createElement('form');
                form.method = 'post';
                form.action = link.getAttribute('href');
                document.body.appendChild(form);
                form.submit();
            });
        })();
