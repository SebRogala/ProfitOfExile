import {createVuetify} from 'vuetify'
import 'vuetify/dist/vuetify.min.css'

const opts = {
    theme: {
        themes: {
            light: {
                colors: {
                    primary: '#1565c0',
                    secondary: '#64b5f6',
                    accent: '#78002e',
                    error: '#d50000',
                }
            },
        }
    }
}

export default createVuetify(opts)
