use topcoat::{
    Result,
    font::fontsource::fontsource_font,
    tailwind,
    view::{component, view},
};

#[component]
pub(crate) async fn document(title: String, child: topcoat::view::View) -> Result {
    view! {
        <!DOCTYPE html>
        <html lang="ja">
            <head>
                <meta charset="utf-8">
                <meta name="viewport" content="width=device-width, initial-scale=1">
                <title>(title)</title>
                topcoat::font::link(font: fontsource_font!(GEIST))
                <link rel="stylesheet" href=(tailwind::stylesheet!())>
                topcoat::runtime::script()
                topcoat::dev::script()
            </head>
            <body class="bg-background text-foreground">
                (child)
            </body>
        </html>
    }
}
