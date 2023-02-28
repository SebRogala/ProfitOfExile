import {createVuetify} from 'vuetify'
import 'vuetify/dist/vuetify.min.css'
import colors from 'vuetify/lib/util/colors'

const opts = {
    theme: {
        defaultTheme: 'dark',
        themes: {
            dark: {
                colors: {
                    primary: colors.indigo.base,
                }
            },
        }
    }
}

export default createVuetify(opts)
