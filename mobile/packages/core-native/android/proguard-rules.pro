# Kept by hand: ubrn's build.gradle names this file, but ubrn writes it only
# for its Kotlin (JNA) bindings, and this package uses JSI. Without it a
# release build fails in mergeReleaseConsumerProguardFiles. The bindings
# reach Rust through C++, so no Java class needs keeping.
