# Python Code Editing

This document serves as a guide to achieve a quality development experience while working with Python in Mudpuppy.


## Visual Studio Code

Before we configure VS Code for writing Python for Mudpuppy, there are two approaches we can take as it pertains to including the required '.pyi' stubs in our VS Code project:

1. Create a 'stubs' folder in your project's root directory and create symbolic links for Mudpuppy's stubs in the directory (recommended)
2. Create a 'stubs' folder in your project's root directory and simply copy Mudpuppy's stubs into the folder (not recommended)

It is recommended that you follow option 1 because of the frequency in which these stubs will change.

The stubs are located at \<mudpuppy-repo-root\>/python-stubs/.

1. Install the '**Python**' extension in VS Code (this should come with the '**Pylance**' extension as well)
2. Click on File -> Options -> Settings
3. Search for the `python.languageServer` option; select '**Pylance**'.
4. Search for the `python.analysis.stubPath` option; put the name of your newly created stubs folder in the option field (a relative path will work).
5. (Optional) Search for the `python.analysis.diagnosticSeverityOverrides` option. We want to suppress 'reportMissingModuleSource'. Note that this is optional, but will clear up any warnings you might see about a missing source module. Here's an example of this section of VS Code's json settings:

```
"python.analysis.diagnosticSeverityOverrides": {
	"reportMissingModuleSource": "none"
}
```
6. Restart VS Code.

