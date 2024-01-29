package cmd

import (
	"fmt"
	"os"

	"github.com/12yanogden/shell"
	sb "github.com/12yanogden/statusbar"
	"github.com/spf13/cobra"
)

// Base command
var rootCmd = &cobra.Command{
	Use:   "umoria",
	Short: "Play the rogue-like crawler, umoria!",
	Long: `Play the rogue-like crawler, umoria!`,

	Run: play,
}

func play(cmd *cobra.Command, args []string) {

	

	// Backup savefile
	var addBar sb.StatusBar
	var commitBar sb.StatusBar
	var pushBar sb.StatusBar

	addBar.Start("Stage local changes")
	shell.Run("git", []string{"add", "."})
	addBar.Pass()

	commitBar.Start("Commit changes to local repository")
	shell.Run("git", []string{"commit", "-m", args[0]})
	commitBar.Pass()

	pushBar.Start("Push changes to remote repository")
	shell.Run("git", []string{"push"})
	pushBar.Pass()
}

func Execute() {
	err := rootCmd.Execute()

	if err != nil {
		os.Exit(1)
	}
}
