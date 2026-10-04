class SingleLineDocstrings():
    ''"Start with empty string" ' and lint docstring safely'
    ''' Not a docstring '''

    def foo(self, bar='''not a docstring'''):
        ''"Start with empty string" ' and lint docstring safely'
        pass

    class Nested(foo()[:]): ''"Start with empty string" ' and lint docstring safely'; pass


class StartWithEmptyStringInPreferredQuotes():
    ""'Start with empty string' ' and lint docstring safely'

    def foo(self):
        ""''' Start with empty string and lint docstring safely '''
        pass
