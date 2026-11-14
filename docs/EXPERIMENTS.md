# Experimental model references

TextIntel previously explored trained similarity/spam scorers, small encoder
heads, and Liquid decision models. Those experiments informed the provider
boundaries and the distinction between evidence and calibrated predictions.
They are not part of the supported library: inference backends, model loaders,
training programs, model catalogs, decision APIs, and trained artifacts were
removed so work can focus on the deterministic text library.

For independent research, primary references include
[Liquid decision models](https://docs.liquid.ai/lfm/models/decision-models),
[Liquid d1-2B](https://docs.liquid.ai/lfm/models/d1-2b), and
[multilingual E5](https://huggingface.co/intfloat/multilingual-e5-small).
These links document external projects, not TextIntel compatibility or an
accuracy guarantee. Any future integration should live outside the core and
be evaluated on an independent task dataset.
