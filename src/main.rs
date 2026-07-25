mod components;

use components::button::button;
use topcoat::{
    Result,
    asset::{AssetBundle, RouterBuilderAssetExt},
    font::fontsource::fontsource_font,
    router::{Router, RouterBuilderDiscoverExt, page},
    tailwind,
    view::{attributes, view},
};

#[tokio::main]
async fn main() {
    let router = Router::builder()
        .discover()
        .assets(AssetBundle::load().unwrap())
        .build();

    topcoat::start(router).await.unwrap();
}

#[page("/")]
async fn home() -> Result {
    view! {
        <!DOCTYPE html>
        <html lang="ja">
            <head>
                <meta charset="utf-8">
                <meta name="viewport" content="width=device-width, initial-scale=1">
                <title>"Lite Vote"</title>
                topcoat::font::link(font: fontsource_font!(GEIST))
                <link rel="stylesheet" href=(tailwind::stylesheet!())>
                topcoat::dev::script()
            </head>
            <body>
                <main class="mx-auto flex min-h-screen max-w-2xl items-center px-6 py-16">
                    <section class="flex w-full flex-col items-start gap-6">
                        <div class="space-y-3">
                            <h1 class="text-4xl font-semibold tracking-tight sm:text-5xl">
                                "Lite Vote"
                            </h1>
                            <p class="max-w-xl text-base leading-7 text-muted-foreground sm:text-lg">
                                "ゲーム中や通話中の「次どうする？」を、その場のみんなで決める投票アプリです。"
                            </p>
                        </div>
                        button(
                            attrs: attributes! {
                                type="button"
                                disabled=(true)
                            },
                            "準備中"
                        )
                    </section>
                </main>
            </body>
        </html>
    }
}
