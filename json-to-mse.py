#!/usr/bin/env python3

import sys

import enum
import io
import mtgjson
import re
import zipfile

CACHE = {
    'db': None,
    'db_x': None
}

COLOR_ABBREVIATIONS = {
    'W': 'White',
    'U': 'Blue',
    'B': 'Black',
    'R': 'Red',
    'G': 'Green'
}

class CommandLineArgs:
    def __init__(self, args=sys.argv[1:]):
        self.verbose = False
        self.cards = set()
        self.output = sys.stdout.buffer
        mode = None
        for arg in args:
            if mode == 'input':
                self.set_input(arg)
                mode = None
            elif mode == 'output':
                self.output = open(arg, 'wb')
                mode = None
            elif arg.startswith('-'):
                if arg.startswith('--'):
                    if arg == '--input':
                        mode = 'input'
                    elif arg.startswith('--input='):
                        self.set_input(arg[len('--input='):])
                    elif arg == '--output':
                        mode = 'output'
                    elif arg.startswith('--output='):
                        self.output = open(arg[len('--output='):], 'wb')
                    elif arg == '--verbose':
                        self.verbose = True
                    else:
                        raise ValueError(f'Unrecognized flag: {arg}')
                else:
                    for i, short_flag in enumerate(arg):
                        if i == 0:
                            continue
                        if short_flag == 'i':
                            if len(arg) > i + 1:
                                self.set_input(arg[i + 1:])
                            else:
                                mode = 'input'
                            break
                        elif short_flag == 'o':
                            if len(arg) > i + 1:
                                self.output = open(arg[i + 1:], 'wb')
                            else:
                                mode = 'output'
                            break
                        elif short_flag == 'v':
                            self.verbose = True
                        else:
                            raise ValueError(f'Unrecognized flag: -{short_flag}')
            else:
                self.cards.add(arg)

    def set_input(self, input_filename):
        with open(input_filename) as f:
            for line in f:
                self.cards.add(line.strip())

class MSEDataFile:
    def __init__(self, data={}):
        self.items = []
        for key, value in data.items():
            self[key] = value

    def __contains__(self, key):
        for iter_key, value in self.items:
            if iter_key == key:
                return True
        else:
            return False

    def __getitem__(self, key):
        result = None
        for iter_key, value in self.items:
            if iter_key == key:
                if result is None:
                    result = value
                else:
                    raise KeyError(f'Multiple values for key {key!r}')
        if result is None:
            raise KeyError(f'No values for key {key!r}')
        else:
            return result

    def __setitem__(self, key, value):
        for iter_key, iter_value in self.items:
            if iter_key == key:
                raise KeyError('Key exists')
        self.add(key, value)

    def __str__(self):
        return self.to_string()

    @classmethod
    def from_card(cls, card_info, db):
        raw_data = card_info._get_raw_data()
        printings = {}
        for set_code, set_info in db.sets.items():
            for iter_card in set_info.cards_by_name.values():
                if iter_card.name == card_info.name:
                    printings[set_code] = iter_card
        result = cls()
        # layout
        if card_info.layout == 'normal':
            pass # nothing specific to normal layout
        else:
            raise NotImplementedError(f'Unsupported layout: {card_info.layout}') #TODO split, flip, double-faced, token, plane, scheme, phenomenon, leveler, vanguard, meld
        # name
        result['name'] = card_info.name
        # mana cost
        if 'manaCost' in raw_data:
            result['casting cost'] = card_info.manaCost
        # color indicator
        if set(raw_data.get('colors', [])) != set(implicit_colors(raw_data.get('manaCost'))):
            pass #TODO
        # type line
        if 'supertypes' in raw_data:
            result['super type'] = f'<word-list-type>{" ".join(card_info.supertypes)} {" ".join(card_info.types)}</word-list-type>'
        else:
            result['super type'] = f'<word-list-type>{" ".join(card_info.types)}</word-list-type>'
        if 'subtypes' in raw_data:
            if 'Creature' in card_info.types:
                card_type = 'race'
            elif 'Instant' in card_info.types or 'Sorcery' in card_info.types:
                card_type = 'spell'
            else:
                card_type = card_info.types[0].lower()
            result['sub type'] = ' '.join(f'<word-list-{card_type}>{subtype}</word-list-race>' for subtype in card_info.subtypes)
        # rarity
        result['rarity'] = min(Rarity.from_str(printing.rarity) for printing in printings.values()).mse_str
        return result

    def add(self, key, value):
        if isinstance(value, dict):
            value = MSEDataFile(value)
        self.items.append((key, value))

    def get(self, key):
        for iter_key, value in self.items:
            if iter_key == key:
                yield value

    def to_string(self, indent=0):
        result = ''
        for key, value in self.items:
            result += '\t' * indent
            if isinstance(value, MSEDataFile):
                result += f'{key}:\r\n'
                result += value.to_string(indent=indent + 1)
            else:
                value_str = str(value)
                if '\n' in value_str:
                    result += f'{key}:\r\n'
                    for line in value_str.split('\n'):
                        result += '\t' * (indent + 1)
                        result += f'{line}\r\n'
                else:
                    result += f'{key}: {value}\r\n'
        return result

class OrderedEnum(enum.Enum):
    def __ge__(self, other):
        if self.__class__ is other.__class__:
            return self.value >= other.value
        return NotImplemented

    def __gt__(self, other):
        if self.__class__ is other.__class__:
            return self.value > other.value
        return NotImplemented

    def __le__(self, other):
        if self.__class__ is other.__class__:
            return self.value <= other.value
        return NotImplemented

    def __lt__(self, other):
        if self.__class__ is other.__class__:
            return self.value < other.value
        return NotImplemented

class Rarity(OrderedEnum):
    BASIC = (0, 'basic land')
    COMMON = (1, 'common')
    UNCOMMON = (2, 'uncommon')
    RARE = (3, 'rare')
    MYTHIC = (4, 'mythic rare')
    SPECIAL = (5, 'special')

    def __init__(self, idx, mse_str):
        self.mse_str = mse_str

    @classmethod
    def from_str(cls, rarity_str):
        return {
            'Basic Land': cls.BASIC,
            'Common': cls.COMMON,
            'Uncommon': cls.UNCOMMON,
            'Rare': cls.RARE,
            'Mythic Rare': cls.MYTHIC,
            'Special': cls.SPECIAL
        }[rarity_str]

def implicit_colors(cost, short=False):
    def cost_part_colors(part):
        basics = '[WUBRG]'
        if re.fullmatch(basics, part):
            # colored mana
            return {COLOR_ABBREVIATIONS[part]}
        if part == 'C':
            # colorless mana
            return set()
        if part == 'S':
            # snow mana
            return set()
        if part == 'X':
            # variable mana
            return set()
        if re.fullmatch('[0-9]+', part):
            # colorless mana
            return set()
        if re.fullmatch('{}/{}'.format(basics, basics), part):
            # colored/colored hybrid mana
            return {COLOR_ABBREVIATIONS[half] for half in part.split('/')}
        if re.fullmatch('{}/P'.format(basics), part):
            # Phyrexian mana
            return {COLOR_ABBREVIATIONS[part[0]]}
        if re.fullmatch('2/{}'.format(basics), part):
            # colorless/colored hybrid mana
            return {COLOR_ABBREVIATIONS[part[2]]}
        raise ValueError('Unknown mana cost part: {{{}}}'.format(part))

    if cost is None or cost == '':
        return []
    if cost[0] != '{' or cost[-1] != '}':
        raise ValueError('Cost must start with { and end with }')
    colors = set()
    for part in cost[1:-1].split('}{'):
        colors |= cost_part_colors(part)
    result = []
    for color in ('White', 'Blue', 'Black', 'Red', 'Green'):
        if color in colors:
            if short:
                result.append({
                    'White': 'W',
                    'Blue': 'U',
                    'Black': 'B',
                    'Red': 'R',
                    'Green': 'G'
                }[color])
            else:
                result.append(color)
    return result

def mtg_json(*, extras=False, verbose=False):
    if extras:
        if CACHE['db_x'] is None:
            if verbose:
                print('[....] downloading MTG JSON', end='', flush=True, file=sys.stderr)
            CACHE['db'] = CACHE['db_x'] = mtgjson.CardDb.from_url(mtgjson.ALL_SETS_X_ZIP_URL)
            if verbose:
                print('\r[ ok ]', file=sys.stderr)
        return CACHE['db_x']
    else:
        if CACHE['db'] is None:
            if verbose:
                print('[....] downloading MTG JSON', end='', flush=True, file=sys.stderr)
            CACHE['db'] = mtgjson.CardDb.from_url()
            if verbose:
                print('\r[ ok ]', file=sys.stderr)
        return CACHE['db']

if __name__ == '__main__':
    try:
        args = CommandLineArgs()
    except ValueError as e:
        sys.exit(f'[!!!!] {e.args[0]}')
    if sys.stdin.isatty():
        card_names = args.cards
    else:
        card_names = set(line.strip() for line in sys.stdin) | args.cards
    if len(card_names) == 0:
        sys.exit('[!!!!] missing card name')
    set_file = MSEDataFile()
    set_file['mse version'] = '0.3.8'
    set_file['game'] = 'magic'
    set_file['stylesheet'] = 'm15'
    set_file['set info'] = {
        'title': 'MTG JSON card import',
        'description': '{} automatically imported from MTG JSON using json-to-mse.'.format('This card was' if len(card_names) == 1 else 'These cards were'),
        'set language': 'EN'
    }
    db = mtg_json(verbose=args.verbose)
    failed = 0
    for i, card_name in enumerate(sorted(card_names)):
        if args.verbose:
            progress = min(4, 5 * i // len(card_names))
            print('[{}{}] adding cards to set file: {} of {}'.format('=' * progress, '.' * (4 - progress), i, len(card_names)), end='\r', flush=True, file=sys.stderr)
        card = db.cards_by_name[card_name]
        try:
            set_file.add('card', MSEDataFile.from_card(card, db))
        except Exception as e:
            if args.verbose:
                raise RuntimeError(f'Failed to add card {card_name}') from e
            else:
                print(f'[ !! ] Failed to add card {card_name}        ', file=sys.stderr)
                failed += 1
    if failed > 0:
        print('[ ** ] Run again with --verbose for a detailed error message', file=sys.stderr)
    if args.verbose:
        print('[ ok ] adding cards to set file: {0} of {0}'.format(len(card_names)), file=sys.stderr)
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, 'x') as f:
        f.writestr('set', str(set_file))
    args.output.write(buf.getvalue())
    args.output.flush()
