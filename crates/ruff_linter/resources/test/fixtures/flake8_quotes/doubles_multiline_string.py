s = """ This "should"
be
"linted" """

s = ''' This "should"
"not" be
"linted" '''

s = """'This should not be linted due to having would-be quadruple end quote'"""

s = ''"""This should be linted, and fixed without a would-be quintuple start quote"""
