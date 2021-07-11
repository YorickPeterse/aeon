from pygments.lexer import RegexLexer, words, bygroups
from pygments import token

__version__ = '1.0.0'

KEYWORDS = (
    'as', 'def', 'do', 'else', 'for', 'impl', 'fn', 'mut', 'class',
    'return', 'self', 'static', 'throw', 'trait', 'try', 'when', 'match',
    'yield', 'local', 'extern', 'ref', 'move'
)


class InkoLexer(RegexLexer):
    name = 'Inko'
    aliases = ['inko']
    filenames = ['*.inko']

    tokens = {
        'root': [
            (r'#.*$', token.Comment.Single),

            ('"', token.String.Double, 'dstring'),
            ("'", token.String.Single, 'sstring'),
            ("`", token.String.Double, 'tstring'),

            (r'_?[A-Z]\w*', token.Name.Constant),
            (r'@_?\w+', token.Name.Variable.Instance),

            (r'(?i)-?0x[0-9a-f_]+', token.Number.Integer),
            (r'(?i)-?[\d_]+\.\d+(e[+-]?\d+)?', token.Number.Float),
            (r'(?i)-?[\d_]+(e[+-]?\d+)?', token.Number.Integer),

            (r'(\w+)(::)', bygroups(token.Name.Namespace, token.Text)),
            (r'\w+:', token.String.Symbol),

            (r'(->|!!|=>)', token.Keyword),
            (r'((<|>|\+|-|\/|\*)=?|==)', token.Operator),

            ('try!', token.Keyword),
            ('import', token.Keyword.Namespace),
            ('let', token.Keyword.Declaration),
            (words(KEYWORDS, suffix=r'\b'), token.Keyword),

            (r'!|\?|\}|\{|\[|\]|\.|,|:|\(|\)|=', token.Punctuation),

            (r'\w+\b', token.Text),
            (r'\s+', token.Whitespace)
        ],
        'dstring': [
            (r'[^"\\]+', token.String.Double),
            (r'\\.', token.String.Escape),
            ('"', token.String.Double, '#pop')
        ],
        'sstring': [
            (r"[^'\\]+", token.String.Single),
            (r"\\.", token.String.Escape),
            ("'", token.String.Single, '#pop')
        ],
        'tstring': [
            (r'[^`\\]+', token.String.Double),
            (r'\\.', token.String.Escape),
            ('`', token.String.Double, '#pop'),
        ]
    }
