use topcoat::{
    Result,
    view::{component, view},
};

#[cfg(not(test))]
#[component]
async fn document_assets() -> Result {
    use topcoat::{font::fontsource::fontsource_font, tailwind};

    view! {
        topcoat::font::link(font: fontsource_font!(GEIST))
        <link rel="stylesheet" href=(tailwind::stylesheet!())>
        topcoat::runtime::script()
        topcoat::dev::script()
    }
}

#[cfg(test)]
#[component]
async fn document_assets() -> Result {
    view! {}
}

#[component]
pub(crate) async fn document(title: String, child: topcoat::view::View) -> Result {
    view! {
        <!DOCTYPE html>
        <html lang="ja">
            <head>
                <meta charset="utf-8">
                <meta name="viewport" content="width=device-width, initial-scale=1">
                <title>(title)</title>
                document_assets()
            </head>
            <body class="bg-background text-foreground">
                (child)
            </body>
        </html>
    }
}
