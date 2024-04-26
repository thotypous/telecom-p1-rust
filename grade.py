import sys
import re
import json
scores = {
    'uart_trivial_48000': 0.5,
    'uart_trivial_44100': 0.5,
    'uart_unsync_44100': 0.5,
    'uart_unsync_48000': 0.5,
    'uart_noisy_48000': 0.5,
    'uart_noisy_44100': 0.5,
    'uart_noisy_unsync_48000': 0.5,
    'uart_noisy_unsync_44100': 0.5,
    'v21_sync_48000': 1,
    'v21_sync_44100': 1,
    'v21_unsync_48000': 1,
    'v21_unsync_44100': 1,
}
results = {}
for line in sys.stdin:
    sys.stdout.write(line)
    m = re.match(r'^test (.*?) \.\.\. (ok|FAILED)', line)
    if m:
        test_name, test_result = m.groups()
        if test_name in scores:
            results[test_name] = scores[test_name] if test_result == 'ok' else 0
print(json.dumps({'scores':results}))
