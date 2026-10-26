import {registerHooks} from 'node:module';

const vendor = new URL('./vendor/', import.meta.url).href;
registerHooks({
    resolve(specifier, context, next) {
        if (context.parentURL?.startsWith(vendor) && specifier.startsWith('.') && !/\.[cm]?[jt]s$/.test(specifier)) {
            try { return next(specifier, context); }
            catch (error) {
                if (error.code !== 'ERR_MODULE_NOT_FOUND') throw error;
                return next(`${specifier}.ts`, context);
            }
        }
        return next(specifier, context);
    },
});
