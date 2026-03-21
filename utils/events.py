"""Utilities for parsing position strings into controller categories."""

def parse_position(position_str: str) -> str:
    """Return a normalized controller category for a given position string.

    Precedence:
      1. Custom mappings in CUSTOM_POSITION_MAP (exact raw match, token match,
         or letters-only match).
      2. Last-three-letters match against the known categories.
      3. Last token match against the known categories.
      4. Default: "APP".
    """

    import re

    # Edit this dict to add custom mappings (case-insensitive).
    # Keys can be the full raw string, a token, or the letters-only concatenation.
    CUSTOM_POSITION_MAP = {"PCT CIC": "CIC"}

    if not position_str:
        return "APP"

    s = position_str.strip().upper()

    # Check custom mappings first (highest priority).
    if CUSTOM_POSITION_MAP:
        norm_map = {k.upper(): v for k, v in CUSTOM_POSITION_MAP.items()}
        if s in norm_map:
            return norm_map[s]

        tokens = re.split(r"[^A-Z]+", s)
        for tok in reversed(tokens):
            if tok and tok in norm_map:
                return norm_map[tok]

        letters = re.findall(r"[A-Z]", s)
        letters_str = "".join(letters)
        if letters_str in norm_map:
            return norm_map[letters_str]
    else:
        letters = re.findall(r"[A-Z]", s)

    if not letters:
        return "APP"

    # Check last three letters (e.g. CTR, APP, TWR, etc.).
    last3 = "".join(letters[-3:])
    valid = {"CTR", "APP", "TWR", "GND", "RMP", "DEL", "CIC", "TMU"}
    if last3 in valid:
        return last3

    # Fallback: scan tokens from right-to-left for a valid category.
    tokens = re.split(r"[^A-Z]+", s)
    for tok in reversed(tokens):
        if tok in valid:
            return tok

    return "APP"

if __name__ == "__main__":
    print(parse_position("IAD_APP"))
    print(parse_position("IAD_M_APP"))
    print(parse_position("DC_CTR"))
    print(parse_position("KRANT + TYSON"))
    print(parse_position("CIC"))
    print(parse_position("PCT CIC"))