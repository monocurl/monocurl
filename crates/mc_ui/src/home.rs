use crate::home::logo::logo;
use crate::home::projects::projects;
use crate::theme::TRANSLUCENT_PURPLE;
use crate::IVP;
use quarve::prelude::*;
use quarve::view::text::TextModifier;

mod logo {
    use crate::IVP;
    use quarve::prelude::*;
    use quarve::view::image_view::ImageView;
    use quarve::view::text::TextModifier;

    pub fn logo() -> impl IVP {
        vstack()
            .push(
                ImageView::named("media/monocurl-1024.png")
                    .padding(10)
            )
            .push(
                HStack::hetero_options(HStackOptions::default().spacing(0.5))
                    .push(
                        text("Monocurl")
                            .text_size(22)
                    )
                    .push(
                        text("[version 0.2.0 - beta]")
                            .text_size(12)
                    )
            )
            .frame(
                F.intrinsic(100, 100)
                    .stretched(400, 400)
            )
            .frame(F.unlimited_width())
    }
}

mod projects {
    use crate::theme::{SUPER_DARK_GRAY, TRANSLUCENT_PURPLE};
    use crate::IVP;
    use quarve::prelude::*;
    use quarve::view::scroll::ScrollView;
    use quarve::view::text::TextModifier;

    fn header() -> impl IVP {
        hstack()
            .push(
                text("Projects")
                    .text_size(24)
            )
            .push(button("New", |s| {

            }).text_color(BLUE)
            )
            .push(button("Import", |s| {

            }).text_color(BLUE))
            .padding(15)
    }

    fn list() -> impl IVP {
        let projects = text("test");

        ScrollView::vertical(
            projects
        )
            .frame(F.unlimited_stretch())
            .padding(10)
    }

    pub fn projects() -> impl IVP {
        VStack::hetero_options(VStackOptions::default().spacing(0.0))
            .push(header())
            .push(
                TRANSLUCENT_PURPLE
                    .frame(F.intrinsic(1,1).unlimited_width())
            )
            .push(list())
            .frame(F.unlimited_stretch())
            .bg_color(SUPER_DARK_GRAY)
    }
}

pub fn home() -> impl IVP {
    HStack::hetero_options(HStackOptions::default().spacing(0.0))
        .push(
            logo()
                .frame(
                    F.intrinsic(300, 500)
                        .squished(300, 500)
                        .unlimited_stretch()
                )
        )
        .push(
            TRANSLUCENT_PURPLE
                .frame(F.intrinsic(1, 1).unlimited_height())
        )
        .push(
            projects()
                .frame(
                    F.intrinsic(300, 500)
                        .squished(300, 500)
                        .unlimited_stretch()
                )
        )
        .bg_color(BLACK)
        .text_color(WHITE)
        .frame(F.unlimited_stretch())
}
