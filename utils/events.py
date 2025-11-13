def parse_position(position_str: str) -> str:
    position_str = position_str.strip().split("_")
    if len(position_str) > 2:
        category = position_str[2]
        return category
    elif len(position_str) == 2:
        category = position_str[1]
        return category
    else:
        return "UNKNOWN"

if __name__ == "__main__":
    print(parse_position("IAD_APP"))
    print(parse_position("IAD_M_APP"))
    print(parse_position("DC_CTR"))