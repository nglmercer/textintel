"""Merge cognate/false-friend features into extracted feature rows.

The two lexicon channels are deterministic functions of the token pair, so
they can be computed without re-running the Rust extractor — the inventory
and normalization here must stay byte-identical with
`src/resources/cognates.rs` (verify with the resource's unit tests).
"""
import json, sys

FALSE_FRIENDS = [("red","red"),("pie","pie"),("pan","pan"),("gift","gift"),("rat","rat"),("mist","mist"),("rope","ropa"),("soap","sopa"),("carpet","carpeta"),("advertisement","advertencia"),("jam","jamon"),("library","libreria"),("large","largo"),("constipated","constipado"),("assist","asistir"),("college","colegio"),("contest","contestar"),("fabric","fabrica"),("idiom","idioma"),("molest","molestar"),("diversion","diversion"),("pretend","pretender"),("become","bekommen"),("bless","blesser"),("hazard","hasard"),("embarrassed","embarazada"),("exit","exito"),("actual","actual"),("sensible","sensible"),("sensitive","sensitivo"),("sympathy","simpatia"),("notice","noticia"),("quit","quitar"),("record","recordar")]
COGNATE_GROUPS = [["internet"],["hotel"],["animal"],["natural"],["final"],["normal"],["popular"],["piano"],["fiesta"],["doctor"],["actor"],["social"],["radio"],["banana"],["video"],["idea"],["paris"],["music","musica","musik"],["hospital","hopital"],["information","informacion"],["chocolate","chocolat"],["guitar","guitarra"],["possible","posible"],["tea","te"],["wine","vino"],["novel","novela"],["person","persona"],["telephone","telefono"],["museum","museo"],["university","universidad"],["computer","computadora"],["important","importante"],["coffee","cafe"],["art","arte"],["geography","geografia"],["poem","poema"],["rose","rosa"],["photo","foto"],["problem","problema"],["system","sistema"],["program","programa"],["moment","momento"],["minute","minuto"],["second","segundo"],["center","centro"],["culture","cultura"],["nature","naturaleza"],["history","historia"],["science","ciencia"],["medicine","medicina"],["television","television"],["taxi"],["metro"],["olive","oliva"],["balcony","balcon"],["vanilla","vainilla"],["pyramid","pyramide"],["family","familia","famille"],["paper","papier"],["soup","soupe"],["lantern","lanterne"],["atom"],["dolphin","delfino","dauphin"],["volcano","vulcano","volcan"],["elephant","elefante"]]

def norm(s):
    out=[]
    for ch in s.lower():
        if not ch.isalnum(): continue
        out.append({'á':'a','à':'a','â':'a','ä':'a','é':'e','è':'e','ê':'e','ë':'e','í':'i','ì':'i','î':'i','ï':'i','ó':'o','ò':'o','ô':'o','ö':'o','ú':'u','ù':'u','û':'u','ü':'u','ñ':'n'}.get(ch,ch))
    return ''.join(out)

FF=set()
for a,b in FALSE_FRIENDS: FF.add((norm(a),norm(b)))
CI={}
for i,g in enumerate(COGNATE_GROUPS):
    for w in g: CI[norm(w)]=i

def ff_pair(a,b):
    ka,kb=norm(a),norm(b); return (ka,kb) in FF or (kb,ka) in FF
def cog(a,b):
    ka,kb=norm(a),norm(b); return ka in CI and kb in CI and CI[ka]==CI[kb]

def merge(src, dst):
    out = open(dst,'w')
    n=0
    for line in open(src):
        try: r = json.loads(line)
        except json.JSONDecodeError: continue
        ta=r['a'].split(); tb=r['b'].split()
        if len(ta)==1 and len(tb)==1:
            r['features']['cognate_match']=1.0 if cog(ta[0],tb[0]) else 0.0
            r['features']['false_friend']=1.0 if ff_pair(ta[0],tb[0]) else 0.0
        else:
            r['features']['cognate_match']=0.0; r['features']['false_friend']=0.0
        out.write(json.dumps(r)+'\n'); n+=1
    out.close()
    print(f"{dst}: {n} rows")

merge(sys.argv[1], sys.argv[2])
