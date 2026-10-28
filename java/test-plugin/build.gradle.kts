plugins { java }
repositories { mavenCentral(); maven("https://repo.papermc.io/repository/maven-public/") }
dependencies { compileOnly("io.papermc.paper:paper-api:26.3.build.227-stable") }
java { toolchain { languageVersion = JavaLanguageVersion.of(25) } }
dependencies { compileOnly(project(":test-dependency")) }
dependencies { compileOnly(project(":")) }
tasks.register<Jar>("legacyJar") {
    archiveClassifier = "legacy"
    from(sourceSets.main.get().output.classesDirs)
    from("src/legacy/resources")
}
tasks.build { dependsOn("legacyJar") }
