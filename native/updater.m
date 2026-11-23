// Main-thread bridge to Sparkle's standard UI and persisted preferences.
#import <Sparkle/Sparkle.h>

static SPUStandardUpdaterController *controller;
static void (*changeHandler)(const char *);
static NSString *pendingVersion;

static void setPendingVersion(NSString *version) {
    pendingVersion = [version copy];
    if (changeHandler) changeHandler(pendingVersion.UTF8String);
}

@interface PlanningUpdateDelegate : NSObject <SPUStandardUserDriverDelegate, SPUUpdaterDelegate>
@end

@implementation PlanningUpdateDelegate
// Background checks never open Sparkle's window, so capture the found version
// here as well as in the user-driver callback used by manual checks.
- (void)updater:(SPUUpdater *)updater didFindValidUpdate:(SUAppcastItem *)item {
    setPendingVersion(item.displayVersionString);
}
- (BOOL)supportsGentleScheduledUpdateReminders { return YES; }
- (BOOL)standardUserDriverShouldHandleShowingScheduledUpdate:(SUAppcastItem *)update andInImmediateFocus:(BOOL)immediateFocus {
    return !immediateFocus;
}
- (void)standardUserDriverWillHandleShowingUpdate:(BOOL)handleShowingUpdate forUpdate:(SUAppcastItem *)update state:(SPUUserUpdateState *)state {
    setPendingVersion(update.displayVersionString);
}
- (void)standardUserDriverWillFinishUpdateSession {
    setPendingVersion(nil);
}
@end

static PlanningUpdateDelegate *delegate;

void planning_updater_set_handler(void (*handler)(const char *)) {
    changeHandler = handler;
    if (changeHandler) changeHandler(pendingVersion.UTF8String);
}

const char *planning_updater_version(void) { return pendingVersion.UTF8String; }

bool planning_updater_start(void) {
    if (controller) return YES;
    delegate = [PlanningUpdateDelegate new];
    controller = [[SPUStandardUpdaterController alloc] initWithStartingUpdater:NO
        updaterDelegate:delegate userDriverDelegate:delegate];
    NSError *error = nil;
    BOOL started = [controller.updater startUpdater:&error];
    if (!started) NSLog(@"Could not start updater: %@", error);
    return started;
}

bool planning_updater_available(void) { return controller != nil; }

bool planning_updater_can_check(void) {
    if (!controller) return NO;
    return pendingVersion != nil || controller.updater.canCheckForUpdates;
}

void planning_updater_check(void) {
    if (planning_updater_can_check()) [controller checkForUpdates:nil];
}

// Silent check used once at launch. Sparkle's gentle scheduled reminders keep
// this from interrupting the user; a manual check still opens its standard UI
// with the release notes and install action.
void planning_updater_check_in_background(void) {
    if (controller) [controller.updater checkForUpdatesInBackground];
}

bool planning_updater_automatic(void) {
    return controller != nil && controller.updater.automaticallyChecksForUpdates;
}

void planning_updater_set_automatic(bool enabled) {
    if (controller) controller.updater.automaticallyChecksForUpdates = enabled;
}
