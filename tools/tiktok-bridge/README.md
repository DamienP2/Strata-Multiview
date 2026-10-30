# Passerelle TikTok Live

TikTok ne propose pas d’API pour lire les commentaires d’un direct. Cette
passerelle les lit avec la bibliothèque libre
[TikTokLive](https://github.com/isaackogan/TikTokLive) et les envoie à Strata
Graphics (commentaires et cadeaux).

Dans Strata : menu principal › Données › Réseaux sociaux › **+** › TikTok
Live, enregistrer, puis copier la commande affichée (elle contient l’adresse
du webhook de la connexion et son jeton).

Sur un poste qui a Python 3.10 ou plus récent et accès à internet :

```sh
python -m venv tiktok && . tiktok/bin/activate     # Windows : tiktok\Scripts\activate
pip install TikTokLive
python bridge.py @compte "https://serveur/api/social/hook/<id>?token=<jeton>"
```

La passerelle attend que le compte passe en direct, puis se reconnecte seule.
Dans Strata, la connexion est « connectée » tant que la passerelle a appelé
dans la dernière minute, « Hors direct » sinon.
