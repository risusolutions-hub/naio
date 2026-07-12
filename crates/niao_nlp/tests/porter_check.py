try:
    from nltk.stem.porter import PorterStemmer
except ImportError:
    import subprocess
    subprocess.check_call(["pip", "install", "nltk", "-q"])
    from nltk.stem.porter import PorterStemmer

ps = PorterStemmer(mode=PorterStemmer.MARTIN_EXTENSIONS)
words = """caresses ponies ssi communication national conditional rational multiplying
triplicate provision hopeful goodness processing predication collaboration comprehensive
revival deciding communal allowance blasphemous substantive advisory agreement abandonment
absolutely absolution effusion electrical fertilizer generalize generating generation generous
ignorant ignorance negotiate negotiation prejudice reciprocal recognize replacement revolution
successful suspicious symmetrical triangular universal vocalize withdrawal achieving activate
announcement appeal decidedly identifiable precedent questionable sentimental traditional""".split()

for w in words:
    print(f'("{w}", "{ps.stem(w)}"),')
