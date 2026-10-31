rootProject.name = "ferrite-api-impl"
include("test-plugin")
project(":test-plugin").projectDir = file("../test-plugin")
include("test-dependency")
project(":test-dependency").projectDir = file("../test-dependency")
include("test-plugin-typical")
project(":test-plugin-typical").projectDir = file("../test-plugin-typical")
