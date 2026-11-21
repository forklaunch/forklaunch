#[macro_export]
macro_rules! mutable_enum {
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident<$lt:lifetime> {
            $($variant:ident($($ty:tt)*)),*
            $(,)?
        }
    ) => {
        $(#[$meta])*
        $vis enum $name<$lt> {
            $($variant(&$lt $($ty)*)),*
        }

        pastey::paste! {
            $(#[$meta])*
            $vis enum [<Mutable $name>]<$lt> {
                $($variant(&$lt mut $($ty)*)),*
            }
        }
    };
}
