<template>
compose strat
</template>

<script>
export default {
    name: 'ComposeStrategies',
    data() {
        return {
            availableStrategies: [],
        }
    },
    mounted() {
    },
    methods: {
        loadData() {
            this.$api.get('/data/').then(res => {

            });
        }
    }
}
</script>
