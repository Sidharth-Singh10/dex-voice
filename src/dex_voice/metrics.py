"""Reference-aligned metrics: recognition timestamps are not ground-truth word ends."""

from dex_voice.streaming import summarize


def normalized(word: str) -> str:
    return "".join(char for char in word.casefold() if char.isalnum() or char == "'")


def align_words(reference: list[dict], observed: list[dict]) -> dict:
    if not reference:
        raise ValueError("A non-empty word-level reference is required")
    expected = [normalized(row["word"]) for row in reference]
    actual = [normalized(row["word"]) for row in observed]
    # Levenshtein alignment, including substitutions/deletions rather than ignoring them.
    costs = [list(range(len(actual) + 1))]
    for i, word in enumerate(expected, 1):
        row = [i]
        for j, candidate in enumerate(actual, 1):
            row.append(
                min(costs[i - 1][j] + 1, row[j - 1] + 1, costs[i - 1][j - 1] + (word != candidate))
            )
        costs.append(row)
    i, j = len(expected), len(actual)
    matches, substitutions, deletions, insertions = [], 0, 0, 0
    while i or j:
        if i and j and costs[i][j] == costs[i - 1][j - 1] + (expected[i - 1] != actual[j - 1]):
            if expected[i - 1] == actual[j - 1]:
                matches.append((i - 1, j - 1))
            else:
                substitutions += 1
            i, j = i - 1, j - 1
        elif i and costs[i][j] == costs[i - 1][j] + 1:
            deletions += 1
            i -= 1
        else:
            insertions += 1
            j -= 1
    return {
        "matches": list(reversed(matches)),
        "substitutions": substitutions,
        "deletions": deletions,
        "insertions": insertions,
        "word_error_rate": costs[-1][-1] / len(expected),
    }


def evaluate(reference: list[dict], observed: list[dict]) -> dict:
    alignment = align_words(reference, observed)
    delays = [observed[j]["committed_at"] - reference[i]["end"] for i, j in alignment["matches"]]
    stats = summarize(delays)
    return {
        "alignment": alignment,
        "stt_word_commit_latency": stats,
        "stt_within_500ms": (
            bool(stats) and stats["deadline_misses"] == 0 and alignment["word_error_rate"] == 0
        ),
        # PCM generation/virtual playback are diagnostics, not verified acoustic word output.
        "end_to_end_within_500ms": None,
    }
