/* Temporarily veto mounts only for the explicitly selected macOS disk. */
#include <DiskArbitration/DiskArbitration.h>
#include <CoreFoundation/CoreFoundation.h>
#include <ctype.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

static const char *target;
static pid_t parent_pid;
static int ready = 0;

static DADissenterRef approve(DADiskRef disk, void *context) {
    (void)context;
    DADiskRef whole = DADiskCopyWholeDisk(disk);
    const char *name = whole ? DADiskGetBSDName(whole) : NULL;
    int matches = name && strcmp(name, target) == 0;
    if (whole) CFRelease(whole);
    if (!matches) return NULL;
    fprintf(stderr, "Blocked mount on %s during Cartridge verification\n", target);
    fflush(stderr);
    return DADissenterCreate(kCFAllocatorDefault, kDAReturnNotPermitted,
                            CFSTR("Cartridge verification is keeping this disk unmounted"));
}

static void heartbeat(CFRunLoopTimerRef timer, void *context) {
    (void)timer;
    (void)context;
    if (getppid() != parent_pid) exit(0);
    if (!ready) {
        ready = 1;
        printf("READY %s\n", target);
        fflush(stdout);
    }
}

int main(int argc, char **argv) {
    if (argc != 2 || strncmp(argv[1], "disk", 4) != 0 || !argv[1][4]) return 2;
    for (const char *c = argv[1] + 4; *c; c++) {
        if (!isdigit((unsigned char)*c)) return 2;
    }
    target = argv[1];
    parent_pid = getppid();
    DASessionRef session = DASessionCreate(kCFAllocatorDefault);
    if (!session) return 3;
    DARegisterDiskMountApprovalCallback(session, NULL, approve, NULL);
    DASessionScheduleWithRunLoop(session, CFRunLoopGetCurrent(), kCFRunLoopDefaultMode);
    CFRunLoopTimerRef timer = CFRunLoopTimerCreate(kCFAllocatorDefault,
        CFAbsoluteTimeGetCurrent() + 0.3, 1.0, 0, 0, heartbeat, NULL);
    CFRunLoopAddTimer(CFRunLoopGetCurrent(), timer, kCFRunLoopDefaultMode);
    CFRunLoopRun();
    CFRelease(timer);
    CFRelease(session);
    return 0;
}
