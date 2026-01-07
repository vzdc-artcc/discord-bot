def parse_position(position_str: str) -> str:
    position_str = position_str.strip().split("_")
    if len(position_str) > 2:
        category = position_str[2]
        return category
    elif len(position_str) == 2:
        category = position_str[1]
        return category
    elif position_str == ["DCAFR"] :
        return "APP"
    elif position_str == ["KRANT"]:
        return "APP"
    elif position_str == ["LURAY"]:
        return "APP"
    elif position_str == ["OJAAY"]:
        return "APP"
    elif position_str == ["TYSON"]:
        return "APP"
    elif position_str == ["ASPER"]:
        return "APP"
    elif position_str == ["BARIN"]:
        return "APP"
    elif position_str == ["IADFC"]:
        return "APP"
    elif position_str == ["IADFE"]:
        return "APP"
    elif position_str == ["IADFW"]:
        return "APP"
    elif position_str == ["MANNE"]:
        return "APP"
    elif position_str == ["MULRR"]:
        return "APP"
    elif position_str == ["TAPPA"]:
        return "APP"
    elif position_str == ["RICFR"]:
        return "APP"
    elif position_str == ["FLTRK"]:
        return "APP"
    elif position_str == ["CSIDW"]:
        return "APP"
    elif position_str == ["CSIDE"]:
        return "APP"
    elif position_str == ["CHOWE"]:
        return "APP"
    elif position_str == ["CHOEA"]:
        return "APP"
    elif position_str == ["WOOLY"]:
        return "APP"
    elif position_str == ["GRACO"]:
        return "APP"
    elif position_str == ["BWFIS"]:
        return "APP"
    elif position_str == ["BUFFR"]:
        return "APP"
    elif position_str == ["KRANT + TYSON"]:
        return "APP"
    elif position_str == ["MANNE + BARIN"]:
        return "APP"
    elif position_str == ["TMU"]:
        return "TMU"
    elif position_str == ["CIC"]:
        return "CIC"
    else:
        return "UNKNOWN"

if __name__ == "__main__":
    print(parse_position("IAD_APP"))
    print(parse_position("IAD_M_APP"))
    print(parse_position("DC_CTR"))
    print(parse_position("KRANT + TYSON"))